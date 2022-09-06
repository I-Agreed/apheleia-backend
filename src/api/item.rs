use crate::{
    auth::{Permission, User},
    db::{model, schema::item, tokio::AsyncRunQueryDsl, DbPool},
    id::{self, Id},
    Result,
};

use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use diesel::QueryDsl;
use serde::{Deserialize, Serialize};

pub(crate) fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_item)
        .service(get_items)
        .service(add_item)
        .service(modify_item)
        .service(delete_item);
}

#[get("/items/{id}")]
async fn get_item(
    _: User,
    item_id: web::Path<Id<id::Item>>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    let item = item::table
        .find(*item_id)
        .first::<model::Item>(&pool)
        .await?;
    Result::Ok(HttpResponse::Ok().json(item))
}

#[get("/items")]
async fn get_items(_: User, pool: web::Data<DbPool>) -> impl Responder {
    let items = item::table.load::<model::Item>(&pool).await?;
    Result::Ok(HttpResponse::Ok().json(items))
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(Serialize))]
struct AddItem {
    note: Option<String>,
    archetype: Id<id::Archetype>,
    archetype_data: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
struct AddItemResponse {
    id: Id<id::Item>,
}

#[post("/items")]
async fn add_item(
    user: User,
    request: web::Json<AddItem>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    if user
        .is_authorised_by_archetype(&pool, request.archetype, Permission::Meta)
        .await?
    {
        let request = request.into_inner();
        let id = Id::new();

        let item = model::Item {
            id,
            note: request.note,
            archetype: request.archetype,
            archetype_data: request.archetype_data,
        };

        diesel::insert_into(item::table)
            .values(item)
            .execute(&pool)
            .await?;

        Result::Ok(HttpResponse::Ok().json(AddItemResponse { id }))
    } else {
        Result::Ok(HttpResponse::Forbidden().finish())
    }
}

#[derive(Clone, Debug, Deserialize, AsChangeset)]
#[cfg_attr(test, derive(Serialize))]
#[diesel(table_name = item)]
struct ModifyItem {
    note: Option<String>,
    archetype: Option<Id<id::Archetype>>,
    archetype_data: Option<serde_json::Value>,
}

#[put("/items/{id}")]
async fn modify_item(
    user: User,
    item_id: web::Path<Id<id::Item>>,
    request: web::Json<ModifyItem>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    if user
        .is_authorised_by_item(&pool, *item_id, Permission::Meta)
        .await?
    {
        let request = request.into_inner();

        let target = item::table.find(*item_id);
        // This will not overwrite note:
        // https://github.com/diesel-rs/diesel/issues/885
        diesel::update(target).set(request).execute(&pool).await?;

        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[delete("/items/{id}")]
async fn delete_item(
    user: User,
    item_id: web::Path<Id<id::Item>>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    if user
        .is_authorised_by_item(&pool, *item_id, Permission::Meta)
        .await?
    {
        let target = item::table.find(*item_id);
        diesel::delete(target).execute(&pool).await?;

        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::{self, TestRequest};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_unauthenticated_item_access() {
        let (app, _pool) = crate::test::init_test_service().await;

        let req = TestRequest::get()
            .uri("/items/30d6efc1-f093-4292-af2c-1d5718403d0c")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let req = TestRequest::get().uri("/items").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let req = TestRequest::post()
            .uri("/items")
            .set_json(AddItem {
                note: Some("note".to_owned()),
                archetype: Id::new(),
                archetype_data: serde_json::Value::Null,
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let req = TestRequest::put()
            .uri("/items/30d6efc1-f093-4292-af2c-1d5718403d0c")
            .set_json(ModifyItem {
                note: None,
                archetype: None,
                archetype_data: None,
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let req = TestRequest::delete()
            .uri("/items/30d6efc1-f093-4292-af2c-1d5718403d0c")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_item_authorisation() {
        let (app, _pool) = crate::test::init_test_service().await;

        let admin = crate::test::create_user(&app).await;
        let subject_area_id = crate::test::create_subject_area(&app, "subject area", admin).await;

        let user = crate::test::create_user(&app).await;

        let resp = user
            .request(
                &app,
                TestRequest::get().uri("/items/30d6efc1-f093-4292-af2c-1d5718403d0c"),
            )
            .await;
        assert_eq!(resp.status(), 404);

        let resp = user.request(&app, TestRequest::get().uri("/items")).await;
        assert_eq!(resp.status(), 200);
        let items = test::read_body_json::<Vec<model::Item>, _>(resp).await;
        assert_eq!(items, vec![]);
    }
}
