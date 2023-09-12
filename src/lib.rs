use log::debug;
use metrics::gauge;
use serde_derive::Deserialize;
use serde_this_or_that::{as_f64, as_u64};
use std::fs;

#[derive(Deserialize)]
struct FritzboxConfig {
    user: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SessionInfo {
    #[serde(rename = "SID")]
    sid: String,
    challenge: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisConnectionData {
    ds_count: u8,
    ds_count_second: u8,
    us_count: u8,
    us_count_second: u8,
    ds_rate: String,
    us_rate: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisConnectionDataWrapper {
    connection_data: DocsisConnectionData,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisConnectionDataWrapperWrapper {
    data: DocsisConnectionDataWrapper,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DOCSIS31UpstreamChannelData {
    #[serde(deserialize_with = "as_f64")]
    power_level: f64,
    modulation: String,
    #[serde(rename = "channelID")]
    channel_id: u64,
    frequency: String,
    #[serde(deserialize_with = "as_u64")]
    activesub: u64,
    fft: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DOCSIS30UpstreamChannelData {
    #[serde(deserialize_with = "as_f64")]
    power_level: f64,
    modulation: String,
    #[serde(rename = "channelID")]
    channel_id: u64,
    frequency: String,
    multiplex: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DOCSIS31DownstreamChannelData {
    #[serde(deserialize_with = "as_f64")]
    power_level: f64,
    non_corr_errors: u32,
    modulation: String,
    #[serde(rename = "channelID")]
    channel_id: u64,
    frequency: String,
    #[serde(deserialize_with = "as_u64")]
    mer: u64,
    fft: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DOCSIS30DownstreamChannelData {
    #[serde(deserialize_with = "as_f64")]
    power_level: f64,
    non_corr_errors: u32,
    corr_errors: u32,
    modulation: String,
    #[serde(rename = "channelID")]
    channel_id: u64,
    frequency: String,
    latency: f64,
    #[serde(deserialize_with = "as_f64")]
    mse: f64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisUpstreamChannelData {
    docsis31: Vec<DOCSIS31UpstreamChannelData>,
    docsis30: Vec<DOCSIS30UpstreamChannelData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisDownstreamChannelData {
    docsis31: Vec<DOCSIS31DownstreamChannelData>,
    docsis30: Vec<DOCSIS30DownstreamChannelData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisChannelData {
    channel_ds: DocsisDownstreamChannelData,
    channel_us: DocsisUpstreamChannelData,
    ready_state: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisChannelDataWrapper {
    data: DocsisChannelData,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisStatistics {
    mse_values: Vec<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisStatisticsData {
    docsis_stats: DocsisStatistics,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DocsisStatisticsDataWrapper {
    data: DocsisStatisticsData,
}

const LOGIN_URL: &str = "http://fritz.box/login_sid.lua";
const DATA_URL: &str = "http://fritz.box/data.lua";

// TODO: Make this a struct that includes the validity of the sid, to avoid frequent logins.
async fn login(config: FritzboxConfig) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let login_url = String::from(LOGIN_URL);
    let res = client.get(&login_url)
        .query(&[("username", &config.user)])
        .send()
        .await?;
    assert_eq!(res.status(), 200);
    let content = res.text().await?;
    let info: SessionInfo = serde_xml_rs::from_str(&content)?;
    let inner_response: Vec<u8> = format!("{0}-{1}", info.challenge, config.password)
        .encode_utf16()
        .into_iter()
        .map(|i| i.to_le_bytes())
        .flatten()
        .collect();
    let outer_response: String = format!("{0}-{1:x}", info.challenge, md5::compute(inner_response));
    let res = client.get(&login_url)
        .query(&[("username", &config.user),
                 ("response", &outer_response)])
        .send()
        .await?;
    assert_eq!(res.status(), 200);
    let content = res.text().await?;
    let info: SessionInfo = serde_xml_rs::from_str(&content)?;
    assert!("0000000000000000" != info.sid, "Password incorrect or Fritzbox denied access due to ratelimiting");
    Ok(info.sid)
}

async fn fetch<T: for<'de> serde::Deserialize<'de>>(sid: &str, page: &str) -> Result<T, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let data_url = String::from(DATA_URL);
    let res = client.post(&data_url)
        .form(&[("xhr", "1"),
                ("sid", &sid),
                ("lang", "en"),  // TODO: This doesn't seem to work.
                ("page", page),
                ("xhrId", "all")])
        .send()
        .await?; 
    assert_eq!(res.status(), 200);
    let content = res.text().await?;
    Ok(serde_json::from_str(&content)?)
}

pub async fn fetch_data() {
    debug!("Logging in...");
    let contents = fs::read_to_string("config.toml")
        .expect("Could not read configuration file");
    let config: FritzboxConfig = toml::from_str(&contents)
        .expect("Could not parse configuration file");
    assert_eq!(config.user, "pkern");
    let sid = login(config)
        .await
        .expect("Could not log into Fritzbox");

    debug!("Fetching data...");

    let data = fetch::<DocsisConnectionDataWrapperWrapper>(&sid, "docOv")
        .await
        .expect("Could not fetch channel overview");
    gauge!("docsis_connection_downstream_count", f64::from(data.data.connection_data.ds_count + data.data.connection_data.ds_count_second));
    gauge!("docsis_connection_upstream_count", f64::from(data.data.connection_data.us_count + data.data.connection_data.us_count_second));

    let data = fetch::<DocsisChannelDataWrapper>(&sid, "docInfo")
        .await
        .expect("Could not fetch channel information");
    const CHANNEL: &'static str = "channel";
    const PROTOCOL: &'static str = "protocol";
    for channel in data.data.channel_ds.docsis31.into_iter() {
        const DOCSIS31: &'static str = "docsis31";
        gauge!("docsis_channel_non_correctable_errors", f64::from(channel.non_corr_errors), PROTOCOL => DOCSIS31, CHANNEL => format!("{}", channel.channel_id));
        gauge!("docsis_channel_power_level", channel.power_level, PROTOCOL => DOCSIS31, CHANNEL => format!("{}", channel.channel_id));
        gauge!("docsis_channel_mer", f64::from(u32::try_from(channel.mer).unwrap_or(0)), PROTOCOL => DOCSIS31, CHANNEL => format!("{}", channel.channel_id));
    };
    for channel in data.data.channel_ds.docsis30.into_iter() {
        const DOCSIS30: &'static str = "docsis30";
        gauge!("docsis_channel_non_correctable_errors", f64::from(channel.non_corr_errors), PROTOCOL => DOCSIS30, CHANNEL => format!("{}", channel.channel_id));
        gauge!("docsis_channel_correctable_errors", f64::from(channel.corr_errors), PROTOCOL => DOCSIS30, CHANNEL => format!("{}", channel.channel_id));
        gauge!("docsis_channel_power_level", channel.power_level, PROTOCOL => DOCSIS30, CHANNEL => format!("{}", channel.channel_id));
        gauge!("docsis_channel_mse", channel.mse, PROTOCOL => DOCSIS30, CHANNEL => format!("{}", channel.channel_id));
    };

    let data = fetch::<DocsisStatisticsDataWrapper>(&sid, "docStat")
        .await
        .expect("Could not fetch channel statistics");

    debug!("Fetching complete.")
}
