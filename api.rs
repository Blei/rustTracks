use serialize::Decodable;
use serialize::json;

fn maybe_extract_from_json_object<T: Decodable<json::Decoder, json::Error>>(
        obj: &json::Object, id: &~str) -> Option<T> {
    let found = match obj.find(id) {
        Some(s) => s.clone(),
        None => return None,
    };
    let mut decoder = json::Decoder::new(found);
    Decodable::decode(&mut decoder).ok()
}

fn extract_from_json_object<T: Decodable<json::Decoder, json::Error>>(obj: &json::Object, id: &~str) -> T {
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

#[deriving(Clone)]
pub struct PlayToken {
    pub s: ~str,
}

pub struct Response<T> {
    pub status: ~str,
    pub errors: Option<~str>,
    pub notices: Option<~str>,
    pub logged_in: bool,
    pub api_version: uint,
    pub contents: Option<T>,
}

impl <T> Response<T> {
    fn from_json(json: &json::Json, contents: Option<T>) -> Response<T> {
        let obj = expect_json_object(json);
        Response::from_json_obj(obj, contents)
    }

    fn from_json_obj(obj: &json::Object, contents: Option<T>) -> Response<T> {
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
    pub sq56: ~str,
    pub sq100: ~str,
    pub sq133: ~str,
    pub max133w: ~str,
    pub max200: ~str,
    pub sq250: ~str,
    pub sq500: ~str,
    pub max1024: ~str,
    pub original: ~str,
}

impl CoverUrls {
    pub fn from_json(json: json::Json) -> CoverUrls {
        let mut decoder = json::Decoder::new(json);
        Decodable::decode(&mut decoder).ok().unwrap()
    }
}

#[deriving(Decodable, Clone)]
pub struct Mix {
    pub id: uint,
    pub path: ~str,
    pub web_path: ~str,
    pub name: ~str,
    pub description: ~str,
    pub plays_count: uint,
    pub likes_count: uint,
    pub certification: Option<~str>,
    // TODO parse this...
    pub tag_list_cache: ~str,
    pub duration: uint,
    pub tracks_count: uint,
    pub nsfw: bool,
    pub liked_by_current_user: bool,
    pub cover_urls: CoverUrls,
    // TODO parse this...
    pub first_published_at: ~str,
    pub user_id: uint,
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
pub struct MixSet {
    pub mixes: ~[Mix],
    pub smart_id: ~str,
    pub smart_type: ~str,
    pub path: ~str,
    pub name: ~str,
    pub web_path: ~str,
}

impl MixSet {
    pub fn from_json(json: &json::Json) -> MixSet {
        let obj = expect_json_object(json);
        let mixes_list = match obj.find(&~"mixes") {
            Some(&json::List(ref list)) => list,
            _ => fail!("expected mixes list in mix set, got {:?}", obj)
        };
        let mixes = mixes_list.iter().map(|json| { Mix::from_json(json) }).collect();
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
pub struct Track {
    pub id: uint,
    pub name: ~str,
    pub performer: ~str,
    pub release_name: Option<~str>,
    pub year: Option<int>,
    pub track_file_stream_url: ~str,
    pub buy_link: ~str,
    pub faved_by_current_user: bool,
    pub url: ~str,
}

#[deriving(Decodable)]
pub struct PlayState {
    pub at_beginning: bool,
    pub at_last_track: bool,
    pub at_end: bool,
    pub skip_allowed: bool,
    pub track: Track,
}

impl PlayState {
    pub fn from_json(json: json::Json) -> PlayState {
        let mut decoder = json::Decoder::new(json);
        Decodable::decode(&mut decoder).ok().unwrap()
    }
}

pub fn parse_mix_set_response(json: &json::Json) -> Response<MixSet> {
    let obj = expect_json_object(json);
    let mix_set = obj.find(&~"mix_set").map(|ms| MixSet::from_json(ms));
    Response::from_json(json, mix_set)
}

pub fn parse_play_token_response(json: &json::Json) -> Response<PlayToken> {
    let obj = expect_json_object(json);
    let pt = maybe_extract_from_json_object(obj, &~"play_token").map(|pt| PlayToken { s: pt });
    Response::from_json(json, pt)
}

pub fn parse_play_state_response(json: &json::Json) -> Response<PlayState> {
    let obj = expect_json_object(json);
    debug!("play state json {}", json.to_str());
    let ps = obj.find(&~"set").map(|set| PlayState::from_json(set.clone()));
    Response::from_json(json, ps)
}
