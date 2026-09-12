#! /usr/bin/env bash
set -euo pipefail

# The below script is run on the github actions CI
# Obtain a list of workspace members
workspace_members="$(
  cargo metadata --format-version 1 --no-deps \
    | jq \
      --compact-output \
      --monochrome-output \
      --raw-output \
      '(.workspace_members | sort) as $package_ids | .packages[] | select(IN(.id; $package_ids[])) | .name'
)"

PACKAGES_CHECKED=()
PACKAGES_SKIPPED=()

# If we are running this on a pull request, then only check for packages that are modified
if [[ "${GITHUB_EVENT_NAME:-}" == 'pull_request' ]]; then
  # Obtain the pull request number and files modified in the pull request
  pull_request_number="$(jq --raw-output '.pull_request.number' "${GITHUB_EVENT_PATH}")"
  files_modified="$(
    gh api \
      --header 'Accept: application/vnd.github+json' \
      --header 'X-GitHub-Api-Version: 2022-11-28' \
      --paginate \
      "https://api.github.com/repos/${GITHUB_REPOSITORY}/pulls/${pull_request_number}/files" \
      --jq '.[].filename'
  )"

  while IFS= read -r package_name; do
    # Obtain pipe-separated list of transitive workspace dependencies for each workspace member
    change_paths="$(cargo tree --all-features --no-dedupe --prefix none --package "${package_name}" \
      | grep 'crates/' \
      | sort --unique \
      | awk --field-separator ' ' '{ printf "crates/%s\n", $1 }' | paste -d '|' -s -)"

    # A package must be checked if any of its transitive dependencies (or itself) has been modified
    if grep --quiet --extended-regexp "^(${change_paths})" <<< "${files_modified}"; then
      printf '::debug::Checking `%s` since at least one of these paths was modified: %s\n' "${package_name}" "${change_paths[*]//|/ }"
      PACKAGES_CHECKED+=("${package_name}")
    else
      printf '::debug::Skipping `%s` since none of these paths were modified: %s\n' "${package_name}" "${change_paths[*]//|/ }"
      PACKAGES_SKIPPED+=("${package_name}")
    fi
  done <<< "${workspace_members}"
  printf '::notice::Packages checked: %s; Packages skipped: %s\n' "${PACKAGES_CHECKED[*]}" "${PACKAGES_SKIPPED[*]}"

  packages_checked="$(jq --compact-output --null-input '$ARGS.positional' --args -- "${PACKAGES_CHECKED[@]}")"

  crates_with_features="$(cargo metadata --format-version 1 --no-deps \
    | jq \
      --compact-output \
      --monochrome-output \
      --raw-output \
      --argjson packages_checked "${packages_checked}" \
      '[ ( .workspace_members | sort ) as $package_ids | .packages[] | select( IN( .name; $packages_checked[] ) ) | { name: .name, features: ( .features | keys ) } ]')"
else
  # If we are doing this locally or on merge queue, then check for all the crates
  crates_with_features="$(cargo metadata --format-version 1 --no-deps \
    | jq \
      --compact-output \
      --monochrome-output \
      --raw-output \
      '[ ( .workspace_members | sort ) as $package_ids | .packages[] | select( IN( .id; $package_ids[] ) ) | { name: .name, features: ( .features | keys ) } ]')"
fi

# List of cargo commands that will be executed
all_commands=()

# Crates that expose their own `fred`/`redis-rs` selector features (which just
# forward to `redis_interface/fred` / `redis_interface/redis-rs`) have a hard
# (non-optional) dependency on `redis_interface` with its own default features
# turned off. `redis_interface` requires exactly one of `fred`/`redis-rs` to be
# active -- a real `compile_error!` in `redis_interface/src/lib.rs` otherwise --
# so any *other* feature of these crates tested in isolation below still needs
# one of the two as a baseline, the same way `v1` is already unconditionally
# appended. Detected generically here (crates with both `fred` and `redis-rs`
# in their feature list) rather than hardcoded to one crate name, so this
# keeps working if another crate adopts the same redis-backend-selector
# pattern later.
crates_requiring_redis_backend="$(
  jq --monochrome-output --raw-output \
    --argjson crates_with_features "${crates_with_features}" \
    --null-input \
    '$crates_with_features[]
    | select( ( IN("fred"; .features[]) ) and ( IN("redis-rs"; .features[]) ) )
    | .name'
)"

# Process the metadata to generate the cargo check commands for crates which have v1 features
# We need to always have the v1 feature with each feature
# This is because, no crate should be run without any features
crates_with_v1_feature="$(
  jq --monochrome-output --raw-output \
    --argjson crates_with_features "${crates_with_features}" \
    --null-input \
    '$crates_with_features[]
    | select( IN("v1"; .features[]))  # Select crates with `v1` feature
    | { name, features: ( .features | del( .[] | select( any( . ; test("(([a-z_]+)_)?v2|v1|default") ) ) ) ) }  # Remove specific features to generate feature combinations
    | { name, features: ( .features | map([., "v1"] | join(",")) ) }  # Add `v1` to remaining features and join them by comma
    | .name as $name | .features[] | { $name, features: . }  # Expand nested features object to have package - features combinations
    | "\(.name) \(.features)" # Print out package name and features separated by space'
)"
while IFS=' ' read -r crate features && [[ -n "${crate}" && -n "${features}" ]]; do
  # See `crates_requiring_redis_backend` above: append the `redis-rs`
  # baseline for crates that need one of `fred`/`redis-rs` active to
  # compile at all, unless the feature under test already is one of them.
  if grep --fixed-strings --line-regexp --quiet "${crate}" <<< "${crates_requiring_redis_backend}" \
    && [[ "${features}" != *"redis-rs"* && "${features}" != *"fred"* ]]; then
    features="${features},redis-rs"
  fi
  command="cargo check --all-targets --package \"${crate}\" --no-default-features --features \"${features}\""
  all_commands+=("$command")
done <<< "${crates_with_v1_feature}"

# For crates which do not have v1 feature, we can run the usual cargo hack command
crates_without_v1_feature="$(
  jq --monochrome-output --raw-output \
    --argjson crates_with_features "${crates_with_features}" \
    --null-input \
    '$crates_with_features[] | select(IN("v1"; .features[]) | not ) # Select crates without `v1` feature
    | "\(.name)" # Print out package name'
)"
while IFS= read -r crate && [[ -n "${crate}" ]]; do
  # Corrected 2026-09-12: `--exclude-no-default-features` alone wasn't
  # enough for `redis_interface` (confirmed by a second real failing
  # `Check compilation on MSRV toolchain` run, same E0412 cascade) --
  # `--each-feature` also runs each *other* feature (`deja`,
  # `multitenancy_fallback`, `metrics`) in isolation with
  # `--no-default-features`, so those combinations still have neither
  # `fred` nor `redis-rs` active. `redis_interface` requires exactly one
  # of the two (real `compile_error!` in its own `src/lib.rs` if neither
  # is set); cargo-hack's own `--at-least-one-of` /
  # `--mutually-exclusive-features` flags exist precisely for a
  # mutually-exclusive, exactly-one-required feature pair like this, so
  # scoping them to `redis_interface` specifically (not every crate in
  # this loop, most of which have no such constraint) is the targeted
  # fix rather than a blanket one.
  command="cargo hack check --all-targets --each-feature --exclude-no-default-features --package \"${crate}\""
  if [[ "${crate}" == "redis_interface" ]]; then
    command="${command} --at-least-one-of fred,redis-rs --mutually-exclusive-features fred,redis-rs"
  fi
  all_commands+=("$command")
done <<< "${crates_without_v1_feature}"

if ((${#all_commands[@]} == 0)); then
  echo "There are no commands to be executed"
  exit 0
fi

echo "The list of commands that will be executed:"
printf "%s\n" "${all_commands[@]}"
echo

# Execute the commands
for command in "${all_commands[@]}"; do
  if [[ "${CI:-false}" == "true" && "${GITHUB_ACTIONS:-false}" == "true" ]]; then
    printf '::group::Running `%s`\n' "${command}"
  fi

  bash -c -x "${command}"

  if [[ "${CI:-false}" == "true" && "${GITHUB_ACTIONS:-false}" == "true" ]]; then
    echo '::endgroup::'
  fi
done
