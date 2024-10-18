FROM rust:1.82 as build-stage

COPY Cargo.lock .
COPY Cargo.toml .

RUN mkdir src && \
    echo "fn main() {print!(\"Dummy main\");} // dummy file" > src/main.rs && \
    set -x && cargo build --release && \
    set -x && rm target/release/deps/suedtirol*

# Now add the rest of the project and build the real main
COPY src ./src
RUN set -x && cargo build --release

# Create a minimal docker image 
FROM debian:12-slim as runtime-stage

RUN apt-get -y update && \
    apt-get install -y \
    libpq-dev \
    openssl \
    libssl-dev \
    ca-certificates && \
    apt-get autoremove && \
    echo -n 'Yes, do as I say!' | apt-get remove apt && \
    rm -rf /var

ENV RUST_LOG="error,suedtirol1-tracker-api=info"
COPY --from=build-stage /target/release/suedtirol1-tracker-api /

EXPOSE 8080

CMD ["/suedtirol1-tracker-api"]
