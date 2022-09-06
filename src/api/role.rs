use crate::{
    auth::User,
    db::{
        model,
        schema::{role, role_permissions},
        tokio::AsyncRunQueryDsl,
        DbPool,
    },
    id::{self, Id},
    Result, Root,
};

use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use diesel::{ExpressionMethods, QueryDsl};
use serde::{Deserialize, Serialize};

pub(crate) fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_role)
        .service(get_roles)
        .service(add_role)
        .service(modify_role)
        .service(delete_role);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ArchetypePermission {
    id: Id<id::Archetype>,
    meta: bool,
    loan: bool,
    receive: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GetRoleResponse {
    pub(crate) id: Id<id::Role>,
    pub(crate) name: String,
    pub(crate) subject_area: Id<id::SubjectArea>,
    pub(crate) permissions: Vec<ArchetypePermission>,
}

#[get("/roles/{id}")]
async fn get_role(
    _: User,
    role_id: web::Path<Id<id::Role>>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    let role = role::table
        .find(*role_id)
        .first::<model::Role>(&pool)
        .await?;

    let permissions = role_permissions::table
        .filter(role_permissions::role.eq(*role_id))
        .load::<model::RolePermission>(&pool)
        .await?
        .into_iter()
        .map(|role_permission| ArchetypePermission {
            id: role_permission.archetype,
            meta: role_permission.meta,
            loan: role_permission.loan,
            receive: role_permission.receive,
        })
        .collect();

    let resp = GetRoleResponse {
        id: role.id,
        name: role.name,
        subject_area: role.subject_area,
        permissions,
    };

    Result::Ok(HttpResponse::Ok().json(resp))
}

#[get("/roles")]
async fn get_roles(pool: web::Data<DbPool>, _: User) -> impl Responder {
    let roles = role::table.load::<model::Role>(&pool).await?;
    Result::Ok(HttpResponse::Ok().json(roles))
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(Serialize))]
pub(crate) struct AddRole {
    pub(crate) name: String,
    pub(crate) subject_area: Id<id::SubjectArea>,
    pub(crate) permissions: Vec<ArchetypePermission>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub(crate) struct AddRoleResponse {
    pub(crate) id: Id<id::Role>,
}

#[post("/roles")]
async fn add_role(
    user: User,
    request: web::Json<AddRole>,
    pool: web::Data<DbPool>,
    root: web::Data<Root>,
) -> impl Responder {
    if user.is_root(*root.into_inner()) || user.is_admin_of(&pool, request.subject_area).await? {
        let request = request.into_inner();
        let id = Id::new();

        let role = model::Role {
            id,
            name: request.name,
            subject_area: request.subject_area,
        };

        diesel::insert_into(role::table)
            .values(role)
            .execute(&pool)
            .await?;

        for permission in request.permissions {
            // TODO: Don't do in loop
            let permission = model::RolePermission {
                role: id,
                archetype: permission.id,
                meta: permission.meta,
                loan: permission.loan,
                receive: permission.receive,
            };
            diesel::insert_into(role_permissions::table)
                .values(permission)
                .execute(&pool)
                .await?;
        }

        Result::Ok(HttpResponse::Ok().json(AddRoleResponse { id }))
    } else {
        Result::Ok(HttpResponse::Forbidden().finish())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ModifyRole {
    name: Option<String>,
    subject_area: Option<Id<id::SubjectArea>>,
    permissions: Option<Vec<ArchetypePermission>>,
}

#[derive(Clone, Debug, AsChangeset)]
#[diesel(table_name = role)]
struct ModifyRoleChangeset {
    name: Option<String>,
    subject_area: Option<Id<id::SubjectArea>>,
}

// TODO: Add way to modify role permissions

#[put("/roles/{id}")]
async fn modify_role(
    user: User,
    role_id: web::Path<Id<id::Role>>,
    request: web::Json<ModifyRole>,
    pool: web::Data<DbPool>,
    root: web::Data<Root>,
) -> impl Responder {
    let role_subject_area = role_id.subject_area().first(&pool).await?;
    if user.is_root(*root.into_inner()) || user.is_admin_of(&pool, role_subject_area).await? {
        let request = request.into_inner();
        let target = role::table.find(*role_id);
        let changeset = ModifyRoleChangeset {
            name: request.name,
            subject_area: request.subject_area,
        };
        diesel::update(target).set(changeset).execute(&pool).await?;
        if let Some(permissions) = request.permissions {
            diesel::delete(role_permissions::table)
                .filter(role_permissions::role.eq(*role_id))
                .execute(&pool)
                .await?;
            for permission in permissions {
                // TODO: Don't do in loop
                let permission = model::RolePermission {
                    role: *role_id,
                    archetype: permission.id,
                    meta: permission.meta,
                    loan: permission.loan,
                    receive: permission.receive,
                };
                diesel::insert_into(role_permissions::table)
                    .values(permission)
                    .execute(&pool)
                    .await?;
            }
        }
        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[delete("/roles/{id}")]
async fn delete_role(
    user: User,
    role_id: web::Path<Id<id::Role>>,
    pool: web::Data<DbPool>,
    root: web::Data<Root>,
) -> impl Responder {
    let role_subject_area = role_id.subject_area().first(&pool).await?;
    if user.is_root(*root.into_inner()) || user.is_admin_of(&pool, role_subject_area).await? {
        let target = role::table.find(*role_id);
        diesel::delete(target).execute(&pool).await?;
        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}
