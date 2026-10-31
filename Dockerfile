FROM rust:1-alpine AS builder

# musl for static linking; rusqlite's "bundled" feature also compiles sqlite from C
RUN apk add --no-cache musl-dev

WORKDIR /build
COPY Cargo.toml Cargo.lock ./

# build dependencies against a dummy main so they're cached between code changes
RUN mkdir src && echo "fn main () { }" > src/main.rs
RUN cargo build --release

COPY src src

# touch main.rs so cargo doesn't think the dummy build is newer than the real code
RUN touch src/main.rs && cargo build --release

FROM scratch

WORKDIR /app

COPY --from=builder /build/target/release/papers ./papers
COPY static static

CMD [ "./papers" ]
