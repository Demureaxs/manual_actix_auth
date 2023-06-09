use std::any::{self, Any};

use crate::{
    // authenticate_token::AuthenticationGuard,
    google_oauth::{get_google_user, request_token},
    model::{AppState, LoginUserSchema, QueryCode, RegisterUserSchema, TokenClaims, User},
    response::{FilteredUser, UserData, UserResponse},
};

use actix_web::{
    cookie::{time::Duration as ActixWebDuration, Cookie},
    get, http, post,
    web::{self, to},
    App, HttpResponse, Responder,
};

use chrono::{prelude::*, Duration};
use jsonwebtoken::{encode, EncodingKey, Header};
use reqwest::header::LOCATION;
use uuid::{timestamp, Uuid};

pub fn user_to_response(user: &User) -> FilteredUser {
    FilteredUser {
        id: user.id.to_owned().unwrap(),
        name: user.name.to_owned(),
        email: user.email.to_owned(),
        verified: user.verified.to_owned(),
        photo: user.photo.to_owned(),
        role: user.role.to_owned(),
        provider: user.provider.to_owned(),
        createdAt: user.createdAt.unwrap(),
        updatedAt: user.updatedAt.unwrap(),
    }
}

#[post("/auth/register")]
async fn register_user_handler(
    body: web::Json<RegisterUserSchema>,
    data: web::Data<AppState>,
) -> impl Responder {
    let mut vec = data.db.lock().unwrap();

    let user = vec.iter().find(|user| user.email == body.email);

    if user.is_some() {
        return HttpResponse::Conflict()
            .json(serde_json::json!({"status": "fail", "message": "Email already exists"}));
    }

    let uuid_id = Uuid::new_v4();
    let datetime = Utc::now();

    let user = User {
        id: Some(uuid_id.to_string()),
        name: body.name.to_owned(),
        verified: false,
        email: body.email.to_owned().to_lowercase(),
        provider: "local".to_string(),
        role: "user".to_string(),
        password: "".to_string(),
        photo: "default.png".to_string(),
        createdAt: Some(datetime),
        updatedAt: Some(datetime),
    };

    vec.push(user.to_owned());

    let json_response = UserResponse {
        status: "success".to_string(),
        data: UserData {
            user: user_to_response(&user),
        },
    };

    HttpResponse::Ok().json(json_response)
}

#[post("/auth/login")]
async fn login_user_handler(
    body: web::Json<LoginUserSchema>,
    data: web::Data<AppState>,
) -> impl Responder {
    let vec = data.db.lock().unwrap();
    let user = vec
        .iter()
        .find(|user| user.email == body.email.to_lowercase());

    if user.is_none() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "fail", "message": "Invalid email or password"}));
    }

    let user = user.unwrap().clone();

    if user.provider == "Google" {
        return HttpResponse::Unauthorized()
            .json(serde_json::json!({"status": "fail", "message": "Use Google OAuth2 instead" }));
    }

    let jwt_secret = data.env.jwt_secret.to_owned();
    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(data.env.jwt_max_age)).timestamp() as usize;
    let claims: TokenClaims = TokenClaims {
        sub: user.id.unwrap(),
        iat,
        exp,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )
    .unwrap();

    let cookie = Cookie::build("token", token)
        .path("/")
        .max_age(ActixWebDuration::new(60 * data.env.jwt_max_age, 0))
        .http_only(true)
        .finish();

    HttpResponse::Ok()
        .cookie(cookie)
        .json(serde_json::json!({"status": "success"}))
}

#[get("/sessions/oauth/google")]
async fn google_oauth_handler(
    query: web::Query<QueryCode>,
    data: web::Data<AppState>,
) -> impl Responder {
    let code = &query.code;
    let state = &query.state;

    if code.is_empty() {
        return HttpResponse::Unauthorized().json(
            serde_json::json!({"status": "fail", "message": "Authorization code not provided"}),
        );
    }

    let token_response = request_token(code.as_str(), &data).await;
    if token_response.is_err() {
        let message = token_response.err().unwrap().to_string();
        return HttpResponse::BadGateway()
            .json(serde_json::json!({"status": "fail", "message": message}));
    }

    let token_response = token_response.unwrap();
    let google_user = get_google_user(&token_response.access_token, &token_response.id_token).await;

    if google_user.is_err() {
        let message = google_user.err().unwrap().to_string();
        return HttpResponse::BadGateway()
            .json(serde_json::json!({"status": "fail", "message": message}));
    }

    let google_user = google_user.unwrap();
    let mut vec = data.db.lock().unwrap();
    let email = google_user.email.to_lowercase();
    let user = vec.iter_mut().find(|user| user.email == email);

    let user_id: String;

    if user.is_some() {
        let user = user.unwrap();
        user_id = user.id.to_owned().unwrap();
        user.email = email.to_owned();
        user.photo = google_user.picture;
        user.updatedAt = Some(Utc::now());
    } else {
        let datetime = Utc::now();
        let id = Uuid::new_v4();
        user_id = id.to_owned().to_string();
        let user_data = User {
            id: Some(id.to_string()),
            name: google_user.name,
            verified: google_user.verified_email,
            email,
            provider: "Google".to_string(),
            role: "user".to_string(),
            password: "".to_string(),
            photo: google_user.picture,
            createdAt: Some(datetime),
            updatedAt: Some(datetime),
        };

        vec.push(user_data.to_owned());
    }

    let jwt_secret = data.env.jwt_secret.to_owned();
    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(data.env.jwt_max_age)).timestamp() as usize;

    let claims: TokenClaims = TokenClaims {
        sub: user_id,
        iat,
        exp,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_ref()),
    )
    .unwrap();

    let cookie = Cookie::build("token", token)
        .path("/")
        .max_age(ActixWebDuration::new(60 * data.env.jwt_max_age, 0))
        .http_only(true)
        .finish();

    let frontend_origin = data.env.client_origin.to_owned();
    let mut response = HttpResponse::Found();
    response.append_header((LOCATION, format!("{}{}", frontend_origin, state)));
    response.cookie(cookie);
    response.finish()
}