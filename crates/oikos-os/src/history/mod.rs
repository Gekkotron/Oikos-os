//! Hub-side history: the Postgres pool and schema (`db`), plus (Task 6) the writer and
//! rollup tasks that feed it from `Hub::ingest`.

pub mod db;
