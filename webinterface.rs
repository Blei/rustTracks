use extra::url;
use extra::json;
use std::str;

use http::client::RequestWriter;
use http::method::Get;
use http::headers::request::ExtensionHeader;

use api;

pub fn make_mixes_url(smart_id: &str) -> url::Url {
    from_str(format!("http://8tracks.com/mix_sets/{}.json?include=mixes[likes_count]", smart_id)).unwrap()
}

pub fn make_play_token_url() -> url::Url {
    from_str("http://8tracks.com/sets/new.json").unwrap()
}

pub fn make_play_url(pt: &api::PlayToken, mix: &api::Mix) -> url::Url {
    from_str(format!("http://8tracks.com/sets/{}/play.json?mix_id={}",
                     **pt, mix.id)).unwrap()
}

pub fn make_next_track_url(pt: &api::PlayToken, mix: &api::Mix) -> url::Url {
    from_str(format!("http://8tracks.com/sets/{}/next.json?mix_id={}",
                     **pt, mix.id)).unwrap()
}

pub fn make_report_url(pt: &api::PlayToken, track_id: uint, mix_id: uint) -> url::Url {
    from_str(format!("http://8tracks.com/sets/{}/report.json?track_id={}&mix_id={}",
        **pt, track_id, mix_id)).unwrap()
}

fn get_json_from_url(url: url::Url) -> json::Json {
    let mut request = RequestWriter::new(Get, url);
    request.headers.insert(ExtensionHeader(~"X-Api-Key", api::API_KEY.to_str()));
    request.headers.insert(ExtensionHeader(~"X-Api-Version", api::API_VERSION.to_str()));
    let mut response = match request.read_response() {
        Ok(response) => response,
        Err(_) => fail!("failed to get mixes"),
    };
    let body = response.read_to_end();
    json::from_str(str::from_utf8_slice(body)).unwrap()
}

pub fn get_mix_set(smart_id: &str) -> json::Json {
    get_json_from_url(make_mixes_url(smart_id))
}

pub fn get_play_token() -> json::Json {
    get_json_from_url(make_play_token_url())
}

pub fn get_play_state(pt: &api::PlayToken, mix: &api::Mix) -> json::Json {
    get_json_from_url(make_play_url(pt, mix))
}

pub fn get_next_track(pt: &api::PlayToken, mix: &api::Mix) -> json::Json {
    get_json_from_url(make_next_track_url(pt, mix))
}

/// Ignoring returned json, if it doesn't work, meh
pub fn report_track(pt: &api::PlayToken, track_id: uint, mix_id: uint) {
    let resp = get_json_from_url(make_report_url(pt, track_id, mix_id));
    debug!("reported track, response was {}", resp.to_str());
}
