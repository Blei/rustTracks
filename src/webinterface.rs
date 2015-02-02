use std::fmt;
use std::old_io;
use std::str;

use rustc_serialize::json;

use hyper;
use hyper::header;

use url;

use api;

#[derive(Clone)]
struct ApiVersionHeader;

impl header::Header for ApiVersionHeader {
    fn header_name() -> &'static str { "X-Api-Version" }
    fn parse_header(raw: &[Vec<u8>]) -> Option<ApiVersionHeader> {
        // TODO fix this up, maybe
        None
    }
}

impl header::HeaderFormat for ApiVersionHeader {
    fn fmt_header(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", api::API_VERSION)
    }
}

#[derive(Clone)]
struct ApiKeyHeader;

impl header::Header for ApiKeyHeader {
    fn header_name() -> &'static str { "X-Api-Key" }
    fn parse_header(raw: &[Vec<u8>]) -> Option<ApiKeyHeader> {
        // TODO fix this, maybe
        None
    }
}

impl header::HeaderFormat for ApiKeyHeader {
    fn fmt_header(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", api::API_KEY)
    }
}


fn make_mixes_url(smart_id: &str) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/mix_sets/{}.json?include=mixes[likes_count]",
                           smart_id).as_slice()).unwrap()
}

fn make_play_token_url() -> url::Url {
    url::Url::parse("http://8tracks.com/sets/new.json").unwrap()
}

fn make_play_url(pt: &api::PlayToken, mix: &api::Mix) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/sets/{}/play.json?mix_id={}",
                            pt.s, mix.id).as_slice()).unwrap()
}

fn make_next_track_url(pt: &api::PlayToken, mix: &api::Mix) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/sets/{}/next.json?mix_id={}",
                            pt.s, mix.id).as_slice()).unwrap()
}

fn make_skip_track_url(pt: &api::PlayToken, mix: &api::Mix) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/sets/{}/skip.json?mix_id={}",
                            pt.s, mix.id).as_slice()).unwrap()
}

fn make_report_url(pt: &api::PlayToken, track_id: u32, mix_id: u32) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/sets/{}/report.json?track_id={}&mix_id={}",
                            pt.s, track_id, mix_id).as_slice()).unwrap()
}

pub fn get_data_from_url_str(s: &str) -> hyper::HttpResult<Vec<u8>> {
    let u = url::Url::parse(s).unwrap();
    get_data_from_url(u)
}

fn get_data_from_url(u: url::Url) -> hyper::HttpResult<Vec<u8>> {
    debug!("fetching data from `{}`", u);
    let mut client = hyper::Client::new();
    let mut response = try!(client.get(u)
                            .header(ApiVersionHeader)
                            .header(ApiKeyHeader)
                            .send());
    response.read_to_end().map_err(|io_err| hyper::HttpError::HttpIoError(io_err))
}

fn get_json_from_url(u: url::Url) -> hyper::HttpResult<json::Json> {
    let data = try!(get_data_from_url(u));
    let s = str::from_utf8(&data[]).unwrap();
    debug!("got data: {}", s);
    Ok(json::Json::from_str(s).unwrap())
}

pub fn get_mix_set(smart_id: &str) -> hyper::HttpResult<json::Json> {
    get_json_from_url(make_mixes_url(smart_id))
}

pub fn get_play_token() -> hyper::HttpResult<json::Json> {
    get_json_from_url(make_play_token_url())
}

pub fn get_play_state(pt: &api::PlayToken, mix: &api::Mix) -> hyper::HttpResult<json::Json> {
    get_json_from_url(make_play_url(pt, mix))
}

pub fn get_next_track(pt: &api::PlayToken, mix: &api::Mix) -> hyper::HttpResult<json::Json> {
    get_json_from_url(make_next_track_url(pt, mix))
}

pub fn get_skip_track(pt: &api::PlayToken, mix: &api::Mix) -> hyper::HttpResult<json::Json> {
    get_json_from_url(make_skip_track_url(pt, mix))
}

/// Ignoring returned json, if it doesn't work, meh
pub fn report_track(pt: &api::PlayToken, track_id: u32, mix_id: u32) {
    let resp = get_json_from_url(make_report_url(pt, track_id, mix_id));
    debug!("reported track, response was {:?}", resp);
}
