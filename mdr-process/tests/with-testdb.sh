#!/usr/bin/env bash
#
# Spin up an ephemeral Postgres, load the same schema fixture mdr-db's
# integration tests use, run the mdr-process integration tests against it, and
# tear the container down afterwards. Reuses mdr-db's docker-compose file and
# schema fixture rather than duplicating them -- see mdr-db/tests/with-testdb.sh.
#
#   mdr-process/tests/with-testdb.sh             # run all integration tests
#   mdr-process/tests/with-testdb.sh import      # run one test target
#
# Requires: docker (with compose) and the psql client on PATH.
set -euo pipefail
cd "$(dirname "$0")"

mdr_db_tests="../../mdr-db/tests"
export TEST_DATABASE_URL="postgres://mdr:mdr@localhost:55432/mdr_test"
target="${1:-import}"

docker compose -f "$mdr_db_tests/docker-compose.yml" up -d --wait
trap 'docker compose -f "$mdr_db_tests/docker-compose.yml" down' EXIT

psql "$TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -q -f "$mdr_db_tests/fixtures/schema.sql"

cargo test -p mdr-process --test "$target"
