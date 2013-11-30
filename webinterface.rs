use api;
use extra::url;
use extra::json;
use http::client::RequestWriter;
use http::method::Get;
use http::headers::request::ExtensionHeader;
use std::str;

pub fn make_mixes_url(smart_id: &str) -> url::Url {
    from_str(format!("http://8tracks.com/mix_sets/{}.json?include=mixes[likes_count]", smart_id)).unwrap()
}

pub fn get_mix_set(smart_id: &str) -> json::Json {
    let url = make_mixes_url(smart_id);
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
