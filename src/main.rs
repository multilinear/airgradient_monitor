use serde::Deserialize;
use tokio;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

// Names are based on JSON format
// We need to parse out fields we just toss
#[allow(non_snake_case, dead_code)]
#[derive(Deserialize,Debug)]
pub struct AirGradientData {
    wifi: i32,
    serialno: String,
    rco2: i32,
    pm01: i32,
    pm02: i32,
    pm10: i32,
    pm003Count: i32,
    atmp: f32,
    rhum: i32,
    atmpCompensated: f32,
    rhumCompensated: i32,
    tvocIndex: i32,
    tvocRaw: i32,
    noxIndex: i32,
    noxRaw: i32,
    boot: i32,
    bootCount: i32,
    ledMode: String,
    firmware: String,
    model: String,
}

//***************************** Influx **********************************

// This is a workaround.
// ideally we'd just use a BTreeMap<String,String>, but
// the config crate case squashes keys. Doing it this way
// makes the tag a value, rather than a key.
#[derive(Debug, Deserialize, Clone)]
struct SettingsPair {
    key: String,
    val: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InfluxSettings {
    token: String,
    bucket: String,
    org: String,
    url: String,
    tags: Vec<SettingsPair>,
}

pub struct Influx {
    cfg: InfluxSettings,
    client: Option<influxdb2::Client>,
}

impl Influx {
    pub fn new(cfg: &InfluxSettings) -> Self {
        Self{
            cfg: cfg.clone(),
            client: None,
        }
    }
    pub async fn connect(&mut self) -> Result<()> {
      match self.client {
        Some(_) => Ok(()),
        None =>  {
            use influxdb2::Client;
            let cfg = &self.cfg;
            let client = Client::new(&cfg.url, &cfg.org, &cfg.token);
            println!("connected to InfluxDB at {0:?}", self.cfg.url);
            self.client = Some(client);
            Ok(())
          },
      }
    }
    pub fn disconnect(&mut self) {
        self.client = None;
    }
    //pub async fn write_point(&mut self, data: &Vec<RegData>, names: &Vec<String>) -> Result<()> {
    pub async fn write_point(&mut self, data: &AirGradientData, aqi: u32) -> Result<()> {
        // Automatically reconnect if we're not connected
        self.connect().await?;
        // connect either created client, or errored out
        // so unwrap can't fail here
        let client = self.client.as_mut().unwrap();
        // Build up the list of points
        use influxdb2::models::DataPoint;
        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap();
        let cfg = &self.cfg;
        // Build our point
        let point: DataPoint = cfg.tags.iter().fold(
            DataPoint::builder("airgradient")
               .field("rco2", data.rco2 as i64)
               .field("pm01", data.pm01 as i64)
               .field("pm02", data.pm02 as i64)
               .field("pm10", data.pm10 as i64)
               .field("pm003Count", data.pm003Count as i64)
               .field("temp", data.atmpCompensated as f64)
               .field("humidity", data.rhumCompensated as i64)
               .field("tvoc", data.tvocRaw as i64)
               .field("tvocIndex", data.tvocIndex as i64)
               .field("nox", data.noxRaw as i64)
               .field("noxIndex", data.noxIndex as i64)
               .field("aqi", aqi as i64)
               .tag("firmware", &data.firmware)
               .tag("model", &data.model)
               .tag("serialno", &data.serialno)
               .timestamp(timestamp),
               |p, tag| p.tag(&tag.key, &tag.val)).build()?;
         client.write(&self.cfg.bucket, futures::stream::iter([point])).await?;
         Ok(())
    }
}



#[derive(Debug, Deserialize)]
struct AirGradientSettings {
	url: String,
  delaysecs: u64,
}

#[derive(Debug, Deserialize)]
struct Settings {
	  airgradient: AirGradientSettings,
    influxdb: InfluxSettings,
}

// algo taken from https://en.wikipedia.org/wiki/Air_quality_index#United_States
fn compute_one_aqi(datum: f64, vector: [f64; 7]) -> u32 {
    const AQI: [f64; 7] = [0.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0];
    let mut i = 0;
    while i <= 6 && datum >= vector[i] {
        i = i + 1;
    }
    return (((AQI[i] - AQI[i-1]) / 
    (vector[i] - vector[i-1])) * (datum - vector[i-1]) + AQI[i-1]) as u32;
}

fn compute_aqi(data: &AirGradientData) -> u32 {
    const PM02: [f64; 7] = [0.0, 9.0, 35.4, 55.4, 125.4, 225.4, 325.4];
    const PM10: [f64; 7] = [0.0, 54.0, 154.0, 254.0, 354.0, 424.0, 604.0];
    //const NOX: [f64; 7] = [0.0, 53.0, 100.0, 360.0, 649.0, 1249.0, 2049.0];
    let mut v: u32 = 0;
    use std::cmp;
    v = cmp::max(v, compute_one_aqi(data.pm02 as f64, PM02)); 
    v = cmp::max(v, compute_one_aqi(data.pm10 as f64, PM10)); 
    // noxRaw isn't the right unit
    //v = cmp::max(v, compute_one_aqi(data.noxRaw as f64, NOX)); 
    return v;
}

async fn do_stuff(influx: &mut Influx, request_url: &str) -> Result<()> {
    let data = reqwest::get(request_url).await?.json::<AirGradientData>().await?;
    let aqi = compute_aqi(&data);
    influx.write_point(&data, aqi).await?;
    return Ok(());
}


#[tokio::main]
async fn main() -> Result<()> {
    use std::time::Duration;
		// Read config
    use std::env;
    let args: Vec<String> = env::args().collect();
    let cfgpath =
        if args.len() < 2 {
            "/etc/airgradient_monitor.toml"
        } else {
            &args[1]
        };
    println!("Reading config file {cfgpath:?}");
		let cfg = config::Config::builder()
		.add_source(config::File::new(cfgpath, config::FileFormat::Toml))
		.build()?;
    let settings : Settings = cfg.try_deserialize()?;
    println!("Read settings {settings:?}");
    // connect to influx
    let mut influx = Influx::new(&settings.influxdb);
    influx.connect().await?;
    let request_url = settings.airgradient.url + "/measures/current";
    let mut interval = tokio::time::interval(Duration::from_secs(settings.airgradient.delaysecs));
    println!("Starting");
    loop {
        match do_stuff(&mut influx, &request_url).await {
            Ok(()) => (),
            Err(error) => {
                println!("Error {error:?}");
                // ignore errors here
                influx.disconnect();
            },
        }
        interval.tick().await;
    };
}
