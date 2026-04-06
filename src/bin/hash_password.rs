use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};

fn main() {
    let password = std::env::args()
        .nth(1)
        .expect("usage: hash-password <password>");
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();
    println!("{hash}");
}
