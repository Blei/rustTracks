use extra::json;
use extra::serialize::Decodable;

fn maybe_extract_from_json_object<T: Decodable<json::Decoder>>(
        obj: &json::Object, id: &~str) -> Option<T> {
    let found = match obj.find(id) {
        Some(s) => s.clone(),
        None => return None,
    };
    let mut decoder = json::Decoder::init(found);
    Decodable::decode(&mut decoder)
}

fn extract_from_json_object<T: Decodable<json::Decoder>>(obj: &json::Object, id: &~str) -> T {
    maybe_extract_from_json_object(obj, id).unwrap_or_else(|| {
        fail!("Didn't find id `{}` or an incorrect type in `{}`", *id, json::Object(~obj.clone()).to_str());
    })
}

fn expect_json_object<'a>(json: &'a json::Json) -> &'a json::Object {
    match *json {
        json::Object(~ref obj) => obj,
        _ => fail!("Expected an object, got {:?}", json),
    }
}

pub static API_VERSION: int = 3;

pub static API_KEY: &'static str = "def2ba77d002afeec898674ede24fe10828ad8a5";

pub struct ApiKey(~str);

#[deriving(Clone)]
pub struct PlayToken(~str);

pub struct Response<T> {
    status: ~str,
    errors: Option<~str>,
    notices: Option<~str>,
    logged_in: bool,
    api_version: uint,
    contents: T,
}

impl <T> Response<T> {
    fn from_json(json: &json::Json, contents: T) -> Response<T> {
        let obj = expect_json_object(json);
        Response::from_json_obj(obj, contents)
    }

    fn from_json_obj(obj: &json::Object, contents: T) -> Response<T> {
        Response {
            status: extract_from_json_object(obj, &~"status"),
            errors: maybe_extract_from_json_object(obj, &~"errors"),
            notices: maybe_extract_from_json_object(obj, &~"notices"),
            logged_in: maybe_extract_from_json_object(obj, &~"logged_in").unwrap_or(false),
            api_version: extract_from_json_object(obj, &~"api_version"),
            contents: contents,
        }
    }
}

#[deriving(Decodable, Clone)]
pub struct CoverUrls {
    sq56: ~str,
    sq100: ~str,
    sq133: ~str,
    max133w: ~str,
    max200: ~str,
    sq250: ~str,
    sq500: ~str,
    max1024: ~str,
    original: ~str,
}

impl CoverUrls {
    pub fn from_json(json: json::Json) -> CoverUrls {
        let mut decoder = json::Decoder::init(json);
        Decodable::decode(&mut decoder)
    }
}

#[deriving(Decodable, Clone)]
pub struct Mix {
    id: uint,
    path: ~str,
    web_path: ~str,
    name: ~str,
    description: ~str,
    plays_count: uint,
    likes_count: uint,
    certification: Option<~str>,
    // TODO parse this...
    tag_list_cache: ~str,
    duration: uint,
    tracks_count: uint,
    nsfw: bool,
    liked_by_current_user: bool,
    cover_urls: CoverUrls,
    // TODO parse this...
    first_published_at: ~str,
    user_id: uint,
}

impl Mix {
    pub fn from_json(json: &json::Json) -> Mix {
        let obj = expect_json_object(json);
        Mix {
            id: extract_from_json_object(obj, &~"id"),
            path: extract_from_json_object(obj, &~"path"),
            web_path: extract_from_json_object(obj, &~"web_path"),
            name: extract_from_json_object(obj, &~"name"),
            description: extract_from_json_object(obj, &~"description"),
            plays_count: extract_from_json_object(obj, &~"plays_count"),
            likes_count: extract_from_json_object(obj, &~"likes_count"),
            certification: maybe_extract_from_json_object(obj, &~"certification"),
            tag_list_cache: maybe_extract_from_json_object(obj, &~"tags_list_cache").unwrap_or_default(),
            duration: extract_from_json_object(obj, &~"duration"),
            tracks_count: extract_from_json_object(obj, &~"tracks_count"),
            nsfw: maybe_extract_from_json_object(obj, &~"nsfw").unwrap_or_default(),
            liked_by_current_user: maybe_extract_from_json_object(obj, &~"liked_by_current_user").unwrap_or_default(),
            cover_urls: CoverUrls::from_json(obj.find(&~"cover_urls").unwrap().clone()),
            first_published_at: extract_from_json_object(obj, &~"first_published_at"),
            user_id: extract_from_json_object(obj, &~"user_id"),
        }
    }
}

#[deriving(Decodable)]
struct MixSet {
    mixes: ~[Mix],
    smart_id: ~str,
    smart_type: ~str,
    path: ~str,
    name: ~str,
    web_path: ~str,
}

impl MixSet {
    pub fn from_json(json: &json::Json) -> MixSet {
        let obj = expect_json_object(json);
        let mixes_list = match obj.find(&~"mixes") {
            Some(&json::List(ref list)) => list,
            _ => fail!("expected mixes list in mix set, got {:?}", obj)
        };
        let mixes = mixes_list.map(|json| { Mix::from_json(json) });
        MixSet {
            mixes: mixes,
            smart_id: extract_from_json_object(obj, &~"smart_id"),
            smart_type: extract_from_json_object(obj, &~"smart_type"),
            path: extract_from_json_object(obj, &~"path"),
            name: extract_from_json_object(obj, &~"name"),
            web_path: extract_from_json_object(obj, &~"web_path"),
        }
    }
}

#[deriving(Clone, Decodable)]
struct Track {
    id: uint,
    name: ~str,
    performer: ~str,
    release_name: ~str,
    year: Option<int>,
    track_file_stream_url: ~str,
    buy_link: ~str,
    faved_by_current_user: bool,
    url: ~str,
}

impl Track {
    pub fn from_json(json: json::Json) -> Track {
        let mut decoder = json::Decoder::init(json);
        Decodable::decode(&mut decoder)
    }
}

#[deriving(Decodable)]
struct PlayState {
    at_beginning: bool,
    at_last_track: bool,
    at_end: bool,
    skip_allowed: bool,
    track: Track,
}

impl PlayState {
    pub fn from_json(json: json::Json) -> PlayState {
        let mut decoder = json::Decoder::init(json);
        Decodable::decode(&mut decoder)
    }
}

pub fn parse_mix_set_response(json: &json::Json) -> Response<MixSet> {
    let obj = expect_json_object(json);
    let mix_set = MixSet::from_json(obj.find(&~"mix_set").unwrap());
    Response::from_json(json, mix_set)
}

pub fn parse_play_token_response(json: &json::Json) -> Response<PlayToken> {
    let obj = expect_json_object(json);
    let pt = PlayToken(extract_from_json_object(obj, &~"play_token"));
    Response::from_json(json, pt)
}

pub fn parse_play_state_response(json: &json::Json) -> Response<PlayState> {
    let obj = expect_json_object(json);
    debug!("play state json {}", json.to_str());
    let ps = PlayState::from_json(obj.find(&~"set").unwrap().clone());
    Response::from_json(json, ps)
}
