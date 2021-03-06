use self::models::*;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use std::env;

use crate::models::{JoinResult, PlayResult};
use std::vec::Vec;

pub mod models;
pub mod schema;

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

fn get_memcache_client() -> memcache::Client {
    memcache::Client::connect("memcache://memcached:11211?timeout=10&tcp_nodelay=true").unwrap()
}

pub fn get_all_logs() -> Vec<JoinResult> {
    use self::schema::logs::dsl::*;
    use self::schema::songs::dsl::*;

    let db = establish_connection();

    convert_to_join_result(
        logs.inner_join(songs)
            .order(date.desc())
            .load::<(Log, Song)>(&db)
            .expect("Error loading logs"),
    )
}

pub fn get_all_songs() -> Vec<Song> {
    use self::schema::songs::dsl::*;

    let db = establish_connection();

    songs.load::<Song>(&db).expect("Error loading songs")
}

pub fn get_current() -> Option<JoinResult> {
    let mut client = get_memcache_client();

    if let Some(data) = client.get::<String>("current").unwrap() {
        println!("got current from cache");
        let current: Option<JoinResult> = serde_json::from_str(&data).unwrap_or(None);
        current
    } else {
        println!("got current from database");
        use self::schema::logs::dsl::*;
        use self::schema::songs::dsl::*;

        let db = establish_connection();

        let result = convert_to_join_result(
            logs.inner_join(songs)
                .limit(1)
                .order(date.desc())
                .load::<(Log, Song)>(&db)
                .expect("Error loading logs"),
        )
        .pop();

        client
            .set("current", serde_json::to_string(&result).unwrap(), 60)
            .unwrap();

        result
    }
}

pub fn get_all_plays() -> Vec<PlayResult> {
    use self::schema::logs::dsl::*;
    use self::schema::songs::dsl::*;
    use itertools::Itertools;

    let db = establish_connection();

    let join = songs.left_join(logs);

    let result = join.order_by(id).load::<(Song, Option<Log>)>(&db);

    let mut results: Vec<PlayResult> = Vec::new();
    if let Ok(r) = result {
        for (_, group) in &r.into_iter().group_by(|(s, _)| s.id) {
            let mut vec = group.collect_vec();
            let first_entry = vec.pop().unwrap();
            let mut dates = Vec::new();

            dates.push((
                first_entry.1.as_ref().unwrap().date,
                first_entry.1.as_ref().unwrap().is_new,
            ));

            for d in vec.iter() {
                dates.push((d.1.as_ref().unwrap().date, d.1.as_ref().unwrap().is_new));
            }

            dates.sort_by(|a, b| b.0.cmp(&a.0));

            results.push(PlayResult {
                song: first_entry.0.clone(),
                plays: dates.len(),
                dates,
            });
        }
    }

    results.sort_by(|a, b| b.plays.cmp(&a.plays));

    results
}

pub fn get_plays(song_id: i32) -> Option<PlayResult> {
    let mut client = get_memcache_client();

    if let Some(data) = client.get::<String>(&format!("plays_{}", song_id)).unwrap() {
        println!("got plays of {} from cache", song_id);
        let plays: Option<PlayResult> = serde_json::from_str(&data).unwrap_or(None);
        plays
    } else {
        println!("got plays of {} from database", song_id);
        use self::schema::logs::dsl::*;
        use self::schema::songs::dsl::*;

        let db = establish_connection();

        if let Ok(mut dates) = logs
            .select((date, is_new))
            .order_by(date)
            .filter(song.eq(song_id))
            .load::<(chrono::NaiveDateTime, bool)>(&db)
        {
            dates.sort_by(|a, b| b.0.cmp(&a.0));

            let result = Some(PlayResult {
                song: songs
                    .filter(id.eq(song_id))
                    .first::<Song>(&db)
                    .expect("Error loading song"),
                plays: dates.len(),
                dates,
            });

            client
                .set(
                    &format!("plays_{}", song_id),
                    serde_json::to_string(&result).unwrap(),
                    300,
                )
                .unwrap();

            result
        } else {
            None
        }
    }
}

fn convert_to_join_result(vec: Vec<(Log, Song)>) -> Vec<JoinResult> {
    let mut results = Vec::new();
    for (log, song) in vec.iter() {
        results.push(JoinResult {
            date: log.date,
            song: song.clone(),
            is_new: log.is_new,
        });
    }
    results
}
