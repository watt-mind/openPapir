## Change

Describe the problem and observable result. Link the public issue, if any.

## Verification

List the checks run and results. Identify any checks that could not run.

## Boundaries

Describe any impact on privacy, extraction safety, or verification claims.

## Checklist

- [ ] `./scripts/check.sh` passes locally
- [ ] Every document affected by this change is updated in this pull request
- [ ] Any new error code, flag, field, or exit status is documented in
      `docs/architecture.md` and indexed in `docs/specification.md`
- [ ] `CHANGELOG.md` has an entry under Unreleased
- [ ] Any new fixture is synthetic and listed in `tests/fixtures/README.md`
- [ ] No real correspondence, private path, identifier, or tracker content
      appears in the code, tests, commits, or this description
