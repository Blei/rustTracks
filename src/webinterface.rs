use std::old_io;
use std::str;

use rustc_serialize::json;

use hyper;
use hyper::header;

use url;

use api;

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

fn make_report_url(pt: &api::PlayToken, track_id: usize, mix_id: usize) -> url::Url {
    url::Url::parse(format!("http://8tracks.com/sets/{}/report.json?track_id={}&mix_id={}",
                            pt.s, track_id, mix_id).as_slice()).unwrap()
}

pub fn get_data_from_url_str(s: &str) -> hyper::HttpResult<Vec<u8>> {
    let u = url::Url::parse(s).unwrap();
    get_data_from_url(u)
}

fn get_data_from_url(u: url::Url) -> hyper::HttpResult<Vec<u8>> {
    debug!("fetching data from `{}`", u);
    let cc = header::CacheControl(
        vec![
            header::CacheDirective::Extension(
                "X-Api-Key".to_string(), Some(api::API_KEY.to_string())),
            header::CacheDirective::Extension(
                "X-Api-Version".to_string(), Some(api::API_VERSION.to_string())),
        ]
    );
    let client = hyper::Client::new();
    let response = try!(client.get(u).header(cc).send());
    response.read_to_end().map_err(|io_err| hyper::HttpError::HttpIoError(io_err))
}

fn get_json_from_url(u: url::Url) -> hyper::HttpResult<json::Json> {
    let data = try!(get_data_from_url(u));
    Ok(json::Json::from_str(str::from_utf8(data.as_slice()).unwrap()).unwrap())
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
pub fn report_track(pt: &api::PlayToken, track_id: usize, mix_id: usize) {
    let resp = get_json_from_url(make_report_url(pt, track_id, mix_id));
    debug!("reported track, response was {:?}", resp);
}
