FROM public.ecr.aws/docker/library/rust:trixie as builder

ARG EXTRA_FEATURES=""
ARG VERSION_FEATURE_SET="v1"

# Which `redis_interface` backend is compiled into the binaries:
#   postgres (default) - cache, locks, pub/sub, ... live in Postgres (Supabase);
#                        no Redis server needed. Needs `redis.postgres_url`
#                        (env ROUTER__REDIS__POSTGRES_URL) and the pg_kv_cache /
#                        pg_pubsub_payload migrations. See crates/redis_interface/README.md.
#   redis-rs | fred    - the original Redis backends.
# The build below passes `--no-default-features`, which drops the `redis-rs`
# default that `router`/`storage_impl`/`scheduler`/`drainer`/`redis_interface`
# each declare, and `release` does not re-enable any backend. `redis_interface`
# refuses to compile with none (or more than one) enabled, so exactly one must be
# named here.
ARG KV_BACKEND="postgres"

# Which cargo profile compiles the binaries: `release` (default, production),
# `release-fast` (optimized, no LTO, for development cycles) or `dev`
# (unoptimized). Features are untouched by this choice.
ARG CARGO_BUILD_PROFILE=release

RUN apt-get update \
    && apt-get install -y libpq-dev libssl-dev pkg-config protobuf-compiler

# Copying codebase from current dir to /router dir
# and creating a fresh build
WORKDIR /router

# Disable incremental compilation.
#
# Incremental compilation is useful as part of an edit-build-test-edit cycle,
# as it lets the compiler avoid recompiling code that hasn't changed. However,
# on CI, we're not making small edits; we're almost always building the entire
# project from scratch. Thus, incremental compilation on CI actually
# introduces *additional* overhead to support making future builds
# faster...but no future builds will ever occur in any given CI environment.
#
# See https://matklad.github.io/2021/09/04/fast-rust-builds.html#ci-workflow
# for details.
ENV CARGO_INCREMENTAL=0
# Allow more retries for network requests in cargo (downloading crates) and
# rustup (installing toolchains). This should help to reduce flaky CI failures
# from transient network timeouts or other issues.
ENV CARGO_NET_RETRY=10
ENV RUSTUP_MAX_RETRIES=10
# Don't emit giant backtraces in the CI logs.
ENV RUST_BACKTRACE="short"

COPY . .
RUN cargo build \
    --profile ${CARGO_BUILD_PROFILE} \
    --no-default-features \
    --features release \
    --features ${VERSION_FEATURE_SET} \
    --features ${KV_BACKEND} \
    ${EXTRA_FEATURES}

# Stage the binary at a profile-independent path for the runtime stage
# (cargo places the `dev` profile under `target/debug`). BINARY is consumed
# after the build so the build layer stays shared across images.
ARG BINARY=router
RUN mkdir -p /router/out \
    && cp "/router/target/$([ "${CARGO_BUILD_PROFILE}" = "dev" ] && echo debug || echo "${CARGO_BUILD_PROFILE}")/${BINARY}" "/router/out/${BINARY}"



FROM public.ecr.aws/docker/library/debian:trixie

# Placing config and binary executable in different directories
ARG CONFIG_DIR=/local/config
ARG BIN_DIR=/local/bin

# Copy this required fields config file
COPY --from=builder /router/config/payment_required_fields_v2.toml ${CONFIG_DIR}/payment_required_fields_v2.toml

# RUN_ENV decides the corresponding config file to be used
ARG RUN_ENV=sandbox

# args for deciding the executable to export. three binaries:
# 1. BINARY=router - for main application
# 2. BINARY=scheduler, SCHEDULER_FLOW=consumer - part of process tracker
# 3. BINARY=scheduler, SCHEDULER_FLOW=producer - part of process tracker
ARG BINARY=router
ARG SCHEDULER_FLOW=consumer

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata libpq-dev curl procps

EXPOSE 8080

ENV TZ=Etc/UTC \
    RUN_ENV=${RUN_ENV} \
    CONFIG_DIR=${CONFIG_DIR} \
    SCHEDULER_FLOW=${SCHEDULER_FLOW} \
    BINARY=${BINARY} \
    RUST_MIN_STACK=6291456

RUN mkdir -p ${BIN_DIR}

COPY --from=builder /router/out/${BINARY} ${BIN_DIR}/${BINARY}

# Create the 'app' user and group
RUN useradd --user-group --system --no-create-home --no-log-init app
USER app:app

WORKDIR ${BIN_DIR}

CMD ./${BINARY}
