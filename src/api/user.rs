use crate::{
    auth::User,
    db::{
        model,
        schema::{role, user, user_roles},
        tokio::AsyncRunQueryDsl,
        DbPool,
    },
    id::{self, Id},
    Result, Root,
};

use actix_http::StatusCode;
use actix_web::{delete, get, post, options, web, HttpResponse, Responder};
use diesel::{ExpressionMethods, QueryDsl};
use serde::Serialize;

pub(crate) fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_user)
        .service(add_user)
        .service(add_user_role)
        .service(delete_user_role)
        .service(preflight);
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(Eq, PartialEq, Deserialize))]
struct GetUserResponse {
    roles: Vec<Id<id::Role>>,
}

#[options("/{x:.*}")]
async fn preflight(_: web::Path<String>) -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[get("/users/{id}")]
async fn get_user(_: User, user_id: web::Path<User>, pool: web::Data<DbPool>) -> impl Responder {
    let _ = user::table
        .find(*user_id)
        .select(user::id)
        .first::<User>(&pool)
        .await?;
    let roles = user_roles::table
        .filter(user_roles::user.eq(*user_id))
        .select(user_roles::role)
        .load::<Id<_>>(&pool)
        .await?;
    let response = GetUserResponse { roles };
    Result::Ok(HttpResponse::Ok().json(response))
}

#[post("/users")]
async fn add_user(user: User, pool: web::Data<DbPool>) -> impl Responder {
    let result = diesel::insert_into(user::table)
        .values(user)
        .execute(&pool)
        .await;

    match result {
        Ok(_) => Result::Ok(HttpResponse::Ok()),
        Err(e) => {
            if let crate::Error::Database(diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            )) = e
            {
                // Result::Ok isn't semantically correct in Rust, but the
                // returned status code is still 409 and so the response is
                // semantically correct externally.
                Result::Ok(HttpResponse::Conflict())
            } else {
                Result::Err(e)
            }
        }
    }
}

#[post("/users/{user_id}/roles/{role_id}")]
async fn add_user_role(
    requesting_user: User,
    path: web::Path<(User, Id<id::Role>)>,
    pool: web::Data<DbPool>,
    root: web::Data<Root>,
) -> impl Responder {
    let (user_id, role_id) = path.into_inner();
    let subject_area_id = role::table
        .find(role_id)
        .select(role::subject_area)
        .first::<Id<id::SubjectArea>>(&pool)
        .await?;
    if requesting_user.is_root(*root.into_inner())
        || requesting_user.is_admin_of(&pool, subject_area_id).await?
    {
        let user_role = model::UserRole {
            user: user_id,
            role: role_id,
        };
        diesel::insert_into(user_roles::table)
            .values(user_role)
            .execute(&pool)
            .await?;
        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[delete("/users/{user_id}/roles/{role_id}")]
async fn delete_user_role(
    requesting_user: User,
    path: web::Path<(User, Id<id::Role>)>,
    pool: web::Data<DbPool>,
    root: web::Data<Root>,
) -> impl Responder {
    let (user_id, role_id) = path.into_inner();
    let subject_area_id = role::table
        .find(role_id)
        .select(role::subject_area)
        .first::<Id<id::SubjectArea>>(&pool)
        .await?;
    if requesting_user.is_root(*root.into_inner())
        || requesting_user.is_admin_of(&pool, subject_area_id).await?
    {
        let target = user_roles::table.find((user_id, role_id));
        diesel::delete(target).execute(&pool).await?;
        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::{
        http::header,
        test::{self, TestRequest},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_unauthenticated_user_access() {
        let (app, _pool) = crate::test::init_test_service().await;

        let req = TestRequest::get().uri("/users/1234").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let req = TestRequest::post().uri("/users").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_user_access() {
        let (app, _pool) = crate::test::init_test_service().await;

        let req = TestRequest::get()
            .uri("/users/1234")
            .insert_header((header::AUTHORIZATION, "Bearer 1234"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        let req = TestRequest::post()
            .uri("/users")
            .insert_header((header::AUTHORIZATION, "Bearer 1234"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let req = TestRequest::post()
            .uri("/users")
            .insert_header((header::AUTHORIZATION, "Bearer 1234"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 409);

        let req = TestRequest::get()
            .uri("/users/1234")
            .insert_header((header::AUTHORIZATION, "Bearer 1234"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let req = TestRequest::get()
            .uri("/users/5678")
            .insert_header((header::AUTHORIZATION, "Bearer 1234"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        let req = TestRequest::get()
            .uri("/users/1234")
            .insert_header((header::AUTHORIZATION, "Bearer 5678"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_user_roles() {
        let (app, _pool) = crate::test::init_test_service().await;
        let user = crate::test::create_user(&app).await;
        let subject_area = crate::test::create_subject_area(&app, "subject area 1", user).await;

        let resp = user
            .request(&app, TestRequest::get().uri(&format!("/users/{user}")))
            .await;
        assert_eq!(resp.status(), 200);
        let user_response = test::read_body_json::<GetUserResponse, _>(resp).await;
        assert_eq!(user_response, GetUserResponse { roles: vec![] });

        let role_1 = crate::test::create_role(&app, "role 1", subject_area).await;
        let resp = User::root()
            .request(
                &app,
                TestRequest::post().uri(&(format!("/users/{user}/roles/{role_1}"))),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let resp = user
            .request(&app, TestRequest::get().uri(&format!("/users/{user}")))
            .await;
        assert_eq!(resp.status(), 200);
        let user_response = test::read_body_json::<GetUserResponse, _>(resp).await;
        assert_eq!(
            user_response,
            GetUserResponse {
                roles: vec![role_1]
            }
        );

        let role_2 = crate::test::create_role(&app, "role 2", subject_area).await;
        let resp = User::root()
            .request(
                &app,
                TestRequest::post().uri(&(format!("/users/{user}/roles/{role_2}"))),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let resp = user
            .request(&app, TestRequest::get().uri(&format!("/users/{user}")))
            .await;
        assert_eq!(resp.status(), 200);
        let user_response = test::read_body_json::<GetUserResponse, _>(resp).await;
        assert_eq!(
            user_response,
            GetUserResponse {
                roles: vec![role_1, role_2]
            }
        );

        let resp = User::root()
            .request(
                &app,
                TestRequest::delete().uri(&(format!("/users/{user}/roles/{role_1}"))),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let resp = user
            .request(&app, TestRequest::get().uri(&format!("/users/{user}")))
            .await;
        assert_eq!(resp.status(), 200);
        let user_response = test::read_body_json::<GetUserResponse, _>(resp).await;
        assert_eq!(
            user_response,
            GetUserResponse {
                roles: vec![role_2]
            }
        );

        let resp = User::root()
            .request(
                &app,
                TestRequest::delete().uri(&(format!("/users/{user}/roles/{role_2}"))),
            )
            .await;
        assert_eq!(resp.status(), 200);

        let resp = user
            .request(&app, TestRequest::get().uri(&format!("/users/{user}")))
            .await;
        assert_eq!(resp.status(), 200);
        let user_response = test::read_body_json::<GetUserResponse, _>(resp).await;
        assert_eq!(user_response, GetUserResponse { roles: vec![] });
    }
}
