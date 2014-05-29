use serialize::Decodable;
use serialize::json;

fn maybe_extract_from_json_object<T: Decodable<json::Decoder, json::DecoderError>>(
        obj: &json::Object, id: &String) -> Option<T> {
    let found = match obj.find(id) {
        Some(s) => s.clone(),
        None => return None,
    };
    let mut decoder = json::Decoder::new(found);
    Decodable::decode(&mut decoder).ok()
}

fn extract_from_json_object<T: Decodable<json::Decoder, json::DecoderError>>(
    obj: &json::Object, id: &String) -> T {

    maybe_extract_from_json_object(obj, id).unwrap_or_else(|| {
        fail!("Didn't find id `{}` or an incorrect type in `{}`",
              *id, json::Object(box obj.clone()).to_str());
    })
}

fn expect_json_object<'a>(json: &'a json::Json) -> &'a json::Object {
    match *json {
        json::Object(box ref obj) => obj,
        _ => fail!("Expected an object, got {:?}", json),
    }
}

pub static API_VERSION: int = 3;

pub static API_KEY: &'static str = "def2ba77d002afeec898674ede24fe10828ad8a5";

#[deriving(Clone)]
pub struct PlayToken {
    pub s: String,
}

pub struct Response<T> {
    pub status: String,
    pub errors: Option<String>,
    pub notices: Option<String>,
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
            status: extract_from_json_object(obj, &"status".to_string()),
            errors: maybe_extract_from_json_object(obj, &"errors".to_string()),
            notices: maybe_extract_from_json_object(obj, &"notices".to_string()),
            logged_in: maybe_extract_from_json_object(obj, &"logged_in".to_string()).unwrap_or(false),
            api_version: extract_from_json_object(obj, &"api_version".to_string()),
            contents: contents,
        }
    }
}

#[deriving(Decodable, Clone)]
pub struct CoverUrls {
    pub sq56: String,
    pub sq100: String,
    pub sq133: String,
    pub max133w: String,
    pub max200: String,
    pub sq250: String,
    pub sq500: String,
    pub max1024: String,
    pub original: String,
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
    pub path: String,
    pub web_path: String,
    pub name: String,
    pub description: String,
    pub plays_count: uint,
    pub likes_count: uint,
    pub certification: Option<String>,
    // TODO parse this...
    pub tag_list_cache: String,
    pub duration: uint,
    pub tracks_count: uint,
    pub nsfw: bool,
    pub liked_by_current_user: bool,
    pub cover_urls: CoverUrls,
    // TODO parse this...
    pub first_published_at: String,
    pub user_id: uint,
}

impl Mix {
    pub fn from_json(json: &json::Json) -> Mix {
        let obj = expect_json_object(json);
        Mix {
            id: extract_from_json_object(obj, &"id".to_string()),
            path: extract_from_json_object(obj, &"path".to_string()),
            web_path: extract_from_json_object(obj, &"web_path".to_string()),
            name: extract_from_json_object(obj, &"name".to_string()),
            description: extract_from_json_object(obj, &"description".to_string()),
            plays_count: extract_from_json_object(obj, &"plays_count".to_string()),
            likes_count: extract_from_json_object(obj, &"likes_count".to_string()),
            certification: maybe_extract_from_json_object(obj, &"certification".to_string()),
            tag_list_cache: maybe_extract_from_json_object(obj, &"tags_list_cache".to_string()).unwrap_or_default(),
            duration: extract_from_json_object(obj, &"duration".to_string()),
            tracks_count: extract_from_json_object(obj, &"tracks_count".to_string()),
            nsfw: maybe_extract_from_json_object(obj, &"nsfw".to_string()).unwrap_or_default(),
            liked_by_current_user: maybe_extract_from_json_object(obj, &"liked_by_current_user".to_string()).unwrap_or_default(),
            cover_urls: CoverUrls::from_json(obj.find(&"cover_urls".to_string()).unwrap().clone()),
            first_published_at: extract_from_json_object(obj, &"first_published_at".to_string()),
            user_id: extract_from_json_object(obj, &"user_id".to_string()),
        }
    }
}

#[deriving(Decodable)]
pub struct MixSet {
    pub mixes: Vec<Mix>,
    pub smart_id: String,
    pub smart_type: String,
    pub path: String,
    pub name: String,
    pub web_path: String,
}

impl MixSet {
    pub fn from_json(json: &json::Json) -> MixSet {
        let obj = expect_json_object(json);
        let mixes_list = match obj.find(&"mixes".to_string()) {
            Some(&json::List(ref list)) => list,
            _ => fail!("expected mixes list in mix set, got {:?}", obj)
        };
        let mixes = mixes_list.iter().map(|json| { Mix::from_json(json) }).collect();
        MixSet {
            mixes: mixes,
            smart_id: extract_from_json_object(obj, &"smart_id".to_string()),
            smart_type: extract_from_json_object(obj, &"smart_type".to_string()),
            path: extract_from_json_object(obj, &"path".to_string()),
            name: extract_from_json_object(obj, &"name".to_string()),
            web_path: extract_from_json_object(obj, &"web_path".to_string()),
        }
    }
}

#[deriving(Clone, Decodable)]
pub struct Track {
    pub id: uint,
    pub name: String,
    pub performer: String,
    pub release_name: Option<String>,
    pub year: Option<int>,
    pub track_file_stream_url: String,
    pub buy_link: String,
    pub faved_by_current_user: bool,
    pub url: String,
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
    let mix_set = obj.find(&"mix_set".to_string()).map(|ms| MixSet::from_json(ms));
    Response::from_json(json, mix_set)
}

pub fn parse_play_token_response(json: &json::Json) -> Response<PlayToken> {
    let obj = expect_json_object(json);
    let pt = maybe_extract_from_json_object(obj, &"play_token".to_string()).map(|pt| PlayToken { s: pt });
    Response::from_json(json, pt)
}

pub fn parse_play_state_response(json: &json::Json) -> Response<PlayState> {
    let obj = expect_json_object(json);
    debug!("play state json {}", json.to_str());
    let ps = obj.find(&"set".to_string()).map(|set| PlayState::from_json(set.clone()));
    Response::from_json(json, ps)
}
