use mads::{DatabaseError, HttpError};

fn implicit(database_error: DatabaseError) {
    let _: HttpError = database_error.into();
}

fn main() {}
