use std::{future::Future, sync::Arc};

use crate::{
    auth::{self, Root},
    Error, FuncReturn, Result,
};

use actix_web::{middleware, web::Data, App, HttpServer};
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::MigrationHarness;

/// Entry point for the server.
///
/// Takes in a single paramater, `token_to_id`, a function which converts an
/// unverified token into some verified ID. For example, an OAuth access token
/// into a user ID.
#[inline]
pub async fn serve<Func, Fut>(token_to_id_function: Func, root: Root) -> Result<()>
where
    Func: Fn(String) -> Fut + 'static + Send + Sync + Clone,
    Fut: Future<Output = std::result::Result<u32, Box<dyn std::error::Error>>> + 'static + Send,
{
    let wrapper = move |token| -> FuncReturn { Box::pin(token_to_id_function(token)) };
    let config = auth::Config {
        token_to_id_function: Arc::new(wrapper),
    };
    let manager =
        ConnectionManager::<PgConnection>::new("postgres://postgres:postgres@db/apheleia");
    let db_pool = Pool::new(manager)?;

    // Update database schema
    let mut conn = db_pool.get()?;
    tokio::task::block_in_place(|| -> Result<()> {
        conn.run_pending_migrations(crate::MIGRATIONS)
            .map_err(|_| Error::Migration)?;
        Ok(())
    })?;
    drop(conn);

    let server = HttpServer::new(move || {
        let cors = actix_cors::Cors::default()
            .allow_any_origin()
            .send_wildcard();
        App::new()
            .app_data(Data::new(db_pool.clone()))
            .app_data(Data::new(root))
            .app_data(config.clone())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .configure(crate::api::config)
    })
    .bind("0.0.0.0:8000")
    .map_err(|e| -> Error { e.into() })?;

    println!("Listening on: {:?}", server.addrs());

    server.run().await.map_err(|e| -> Error { e.into() })
}
