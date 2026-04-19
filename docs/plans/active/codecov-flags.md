# Codecov per-crate flags

## Goal

Improve coverage visibility by uploading one Codecov flag per public crate while keeping the overall workspace upload.

## Scope

- generate one Codecov flag report per public crate
- keep the existing overall workspace coverage upload
- update crate README badges to point at each crate's own coverage flag
- lower the Codecov patch coverage target from 100% to 95%

## Checklist

- [x] add a script to split workspace LCOV output into per-crate LCOV files
- [x] update CI coverage jobs to upload the overall report plus one flag per public crate
- [x] update Codecov config with crate flags and a 95% patch target
- [x] update crate README badge links to use per-crate Codecov flags
- [x] run docs regeneration and validate the new coverage flag split locally
