use mads::prelude::*;

fn managed<T>(value: mads::DatabaseResult<T>) -> mads::HttpResult<T> {
    value.into_http()
}

fn native<T>(value: mads::diesel::QueryResult<T>) -> mads::HttpResult<T> {
    value.into_http()
}

fn main() {
    let _ = managed::<()>;
    let _ = native::<()>;
}
