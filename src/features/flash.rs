use axum_extra::extract::cookie::{Cookie, CookieJar};

const COOKIE_NAME: &str = "flash";

pub enum Flash {
    Success(&'static str),
    Error(&'static str),
}

impl Flash {
    fn encode(&self) -> String {
        match self {
            Flash::Success(msg) => format!("s:{msg}"),
            Flash::Error(msg) => format!("e:{msg}"),
        }
    }

    pub fn into_jar(self, jar: CookieJar) -> CookieJar {
        let mut cookie = Cookie::new(COOKIE_NAME, self.encode());
        cookie.set_path("/");
        cookie.set_http_only(false);
        cookie.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
        jar.add(cookie)
    }
}

pub fn redirect(jar: CookieJar, flash: Flash, to: &str) -> axum::response::Response {
    use axum::response::IntoResponse;
    (flash.into_jar(jar), axum::response::Redirect::to(to)).into_response()
}
