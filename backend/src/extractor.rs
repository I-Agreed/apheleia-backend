use std::{future::Future, marker::PhantomData};

type BoxFuture<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'static, std::alloc::Global>>;

pub struct Id<F>(
    pub String,
    // I think this is the correct use of PhantomData. I need a generic for IdConfig in the
    // FromRequest implementation for Id, but if the generic is not directly used by Id,
    // the compiler says that the generic is unconstrained.
    PhantomData<F>,
)
where
    F: Fn(String) -> BoxFuture<Result<String, Box<dyn std::error::Error>>>;

pub struct IdConfig<F>
where
    F: Fn(String) -> BoxFuture<Result<String, Box<dyn std::error::Error>>>,
{
    pub token_to_id_function: F,
}

impl<F> Default for IdConfig<F>
where
    F: Fn(String) -> BoxFuture<Result<String, Box<dyn std::error::Error>>>,
{
    fn default() -> Self {
        panic!("No ID extractor specified");
    }
}

impl<F> actix_web::FromRequest for Id<F>
where
    F: Fn(String) -> BoxFuture<Result<String, Box<dyn std::error::Error>>> + 'static,
{
    type Error = ();

    type Future = BoxFuture<Result<Self, Self::Error>>;

    type Config = IdConfig<F>;

    fn from_request(
        _req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        todo!();
    }
}
