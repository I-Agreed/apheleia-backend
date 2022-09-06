use crate::{
    auth::{Permission, User},
    db::{
        model,
        schema::{archetype, item, loan, role_permissions},
        tokio::AsyncRunQueryDsl,
        DbPool,
    },
    id::{self, Id},
    Result,
};

use actix_web::{delete, get, post, put, web, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use diesel::{query_dsl::JoinOnDsl, ExpressionMethods, QueryDsl};
use serde::{Deserialize, Serialize};

pub(crate) fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_loan)
        .service(get_loans)
        .service(add_loan)
        .service(modify_loan)
        .service(delete_loan);
}

#[get("/loans/{id}")]
async fn get_loan(
    _: User,
    loan_id: web::Path<Id<id::Item>>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    let loan = loan::table
        .find(*loan_id)
        .first::<model::Loan>(&pool)
        .await?;

    Result::Ok(HttpResponse::Ok().json(loan))
}

#[derive(Copy, Clone, Debug, Deserialize)]
struct GetLoans {
    role: GetLoansFilter,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GetLoansFilter {
    Loanee,
    Loaner,
    Manager,
}

#[get("/loans")]
async fn get_loans(
    user: User,
    params: web::Query<GetLoans>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    Result::Ok(match params.role {
        GetLoansFilter::Loanee => {
            let loans = loan::table
                .filter(loan::loanee.eq(user))
                .load::<model::Loan>(&pool)
                .await?;
            HttpResponse::Ok().json(loans)
        }
        GetLoansFilter::Loaner => {
            let loans = loan::table
                .filter(loan::loaner.eq(user))
                .load::<model::Loan>(&pool)
                .await?;
            HttpResponse::Ok().json(loans)
        }
        GetLoansFilter::Manager => {
            let loans = loan::table
                .inner_join(item::table)
                .inner_join(archetype::table.on(item::archetype.eq(archetype::id)))
                .filter(
                    archetype::id.eq_any(user.permissions().select(role_permissions::archetype)),
                )
                .select(loan::all_columns)
                .load::<model::Loan>(&pool)
                .await?;
            HttpResponse::Ok().json(loans)
        }
    })
}

#[derive(Clone, Debug, Deserialize)]
struct AddLoan {
    loanee: User,
    item: Id<id::Item>,
    note: Option<String>,
    date_due: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
struct AddLoanResponse {
    id: Id<id::Loan>,
}

#[post("/loans")]
async fn add_loan(
    user: User,
    request: web::Json<AddLoan>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    if user
        .is_authorised_by_item(&pool, request.item, Permission::Loan)
        .await?
    {
        let request = request.into_inner();
        let id = Id::new();

        let loan = model::Loan {
            id,
            return_requested: false,
            item: request.item,
            loaner: user,
            loanee: request.loanee,
            note: request.note,
            date_loaned: Utc::now(),
            date_due: request.date_due,
            date_returned: None,
        };

        diesel::insert_into(loan::table)
            .values(loan)
            .execute(&pool)
            .await?;
        Result::Ok(HttpResponse::Ok().json(AddLoanResponse { id }))
    } else {
        Result::Ok(HttpResponse::Forbidden().finish())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ModifyLoan {
    return_requested: Option<bool>,
    note: Option<String>,
    date_loaned: Option<DateTime<Utc>>,
    date_due: Option<DateTime<Utc>>,
    returned: Option<bool>,
}

#[put("/loans/{id}")]
async fn modify_loan(
    user: User,
    loan_id: web::Path<Id<id::Loan>>,
    request: web::Json<ModifyLoan>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    if user
        .is_authorised_by_loan(&pool, *loan_id, Permission::Receive)
        .await?
    {
        let request = request.into_inner();

        #[derive(Clone, Debug, Deserialize, AsChangeset)]
        #[diesel(table_name = loan)]
        struct ModifyLoanChangeset {
            return_requested: Option<bool>,
            note: Option<String>,
            date_loaned: Option<DateTime<Utc>>,
            date_due: Option<DateTime<Utc>>,
            date_returned: Option<DateTime<Utc>>,
        }

        let mut changeset = ModifyLoanChangeset {
            return_requested: request.return_requested,
            note: request.note,
            date_loaned: request.date_loaned,
            date_due: request.date_due,
            date_returned: None,
        };

        if request.returned.unwrap_or(false) {
            changeset.date_returned = Some(Utc::now());
            changeset.return_requested = Some(false);
        }

        let target = loan::table.find(*loan_id);
        diesel::update(target).set(changeset).execute(&pool).await?;

        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}

#[delete("/loans/{id}")]
async fn delete_loan(
    user: User,
    loan_id: web::Path<Id<id::Loan>>,
    pool: web::Data<DbPool>,
) -> impl Responder {
    let loan_subject_area = loan_id.subject_area().first(&pool).await?;
    if user.is_admin_of(&pool, loan_subject_area).await? {
        let target = loan::table.find(*loan_id);
        diesel::delete(target).execute(&pool).await?;
        Result::Ok(HttpResponse::Ok())
    } else {
        Result::Ok(HttpResponse::Forbidden())
    }
}
