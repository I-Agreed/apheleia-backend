# Adapted from: https://dev.to/rogertorres/first-steps-with-docker-rust-30oi

# So that matching GLIBC version is used by debian:buster-slim image
FROM rust:slim-buster as build

WORKDIR /build

# create dummy packages
RUN USER=root cargo init --bin --name apheleia

# copy over manifests
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock

# cache dependencies
RUN cargo build --release

# delete dummy binaries
RUN rm ./target/release/deps/apheleia*

# copy source tree
RUN rm ./src/*.rs
COPY ./src ./src

COPY ./migrations ./migrations

RUN apt-get update
RUN apt-get install libpq-dev -y

WORKDIR bin

RUN cargo build --release

FROM debian:buster-slim

RUN apt-get update
RUN apt-get install libpq-dev -y

# copy build artifact from build container
COPY --from=build /build/target/release/apheleia /apheleia

CMD ["/apheleia"]
