#!/usr/bin/env bash
# Single source of truth for release-tag resolution.
#
# `.github/workflows/release.yml` sources this script instead of carrying its
# own tag regex, so the accepted tag grammar and the derived release metadata
# (prerelease flag, release name, Docker tags) live in exactly one place.
#
# Usage (in GitHub Actions):
#   source scripts/release-tag.sh
#   if ! resolve_release_tag "$TAG" > /tmp/release-tag.out; then exit 1; fi
#   cat /tmp/release-tag.out >> "$GITHUB_OUTPUT"
#
# `resolve_release_tag` emits `key=value` lines in the GITHUB_OUTPUT format
# (multiline `docker_tags` via the `<<RS_EOF` delimiter), so the workflow can
# append the output verbatim:
#
#   version=0.8.0-alpha.1
#   is_prerelease=true
#   version_major_minor=0.8
#   version_major=0
#   release_name=Elembra v0.8.0-alpha.1
#   docker_tags<<RS_EOF
#   type=raw,value=0.8.0-alpha.1
#   RS_EOF
#
# Validation: strict SemVer 2.0.0 with a `v` prefix. Leading zeros, empty
# prerelease segments, and stray identifiers are rejected. Build metadata
# (`+meta`) is accepted and does NOT make a tag a prerelease.
#
# `immutability_decision` implements release immutability: a version that
# already has a GitHub release may only be re-run via workflow_dispatch at its
# own commit (repair mode); a tag push against an existing release is a
# force-move/duplicate attempt and is rejected. The workflow calls it in
# validate-tag, before anything is built.
#
# Run `bash scripts/release-tag.sh --selftest` to assert the full matrix.

set -euo pipefail

# Strict SemVer 2.0.0 (numeric core without leading zeros, optional
# dot-separated prerelease, optional dot-separated build metadata) + `v` prefix.
RS_SEMVER_RE='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-((0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*))*))?(\+[0-9a-zA-Z-]+(\.[0-9a-zA-Z-]+)*)?$'

# resolve_release_tag <tag>
#   <tag> must carry the `v` prefix, e.g. `v0.7.0` or `v0.8.0-alpha.1`.
#   Prints GITHUB_OUTPUT-compatible metadata; exits 1 on an invalid tag.
resolve_release_tag() {
	local tag="$1"
	local version core version_major_minor version_major is_prerelease release_name docker_version

	if [[ -z "$tag" ]]; then
		echo "release-tag: error: empty tag (expected e.g. v0.7.0 or v0.8.0-alpha.1)" >&2
		return 1
	fi
	if [[ ! "$tag" =~ $RS_SEMVER_RE ]]; then
		echo "release-tag: error: invalid tag '$tag' (expected strict semver with a v prefix, e.g. v0.7.0 or v0.8.0-alpha.1)" >&2
		return 1
	fi

	version="${tag#v}"
	# Numeric core = version up to the first '-' (prerelease) or '+' (build metadata).
	core="${version%%[-+]*}"
	version_major_minor="${core%.*}"
	version_major="${core%%.*}"
	# Docker tag names cannot contain '+', so tags are derived from the
	# build-metadata-free version (e.g. v1.2.3+meta pushes as `1.2.3`).
	docker_version="${version%%+*}"

	# A '-' immediately after the numeric core is the prerelease separator; a
	# '-' inside build metadata never appears in that position, so this cannot
	# misclassify e.g. v1.2.3+build-1.
	if [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+- ]]; then
		is_prerelease=true
		release_name="Elembra v${version}"
	else
		is_prerelease=false
		release_name="RustShare v${version}"
	fi

	echo "version=$version"
	echo "is_prerelease=$is_prerelease"
	echo "version_major_minor=$version_major_minor"
	echo "version_major=$version_major"
	echo "release_name=$release_name"
	echo "docker_tags<<RS_EOF"
	if [[ "$is_prerelease" == "true" ]]; then
		# Prereleases get ONLY the exact version tag; `latest` and the rolling
		# major/minor aliases must never point at a prerelease. The workflow
		# appends the immutable `sha-<short>` tag on top.
		echo "type=raw,value=$docker_version"
	else
		echo "type=raw,value=$docker_version"
		echo "type=raw,value=$version_major_minor"
		echo "type=raw,value=$version_major"
		echo "type=raw,value=latest"
	fi
	echo "RS_EOF"
}

# Reason used for every immutability FAIL; the workflow surfaces it verbatim.
# Version-class-aware: a defective stable release is fixed by the next stable
# patch, a defective prerelease by the next prerelease version.
RS_IMMUTABILITY_FAIL_REASON="version already released; a defective release must be fixed by the next version of the same class (prerelease->prerelease, stable->patch)"

# immutability_decision <event_name> <tag> <sha> <release_exists> <tag_sha>
#   Pure release-immutability decision for a release attempt:
#     - no existing GitHub release -> ALLOW
#     - existing release + workflow_dispatch + tag_sha == sha
#       -> ALLOW (explicit repair mode: re-running the SAME version at ITS OWN
#          commit, e.g. the v0.7.0 remediation path)
#     - anything else (incl. a tag push against an existing release = force-move
#       or duplicate attempt) -> FAIL:<reason>
#   Echoes ALLOW or FAIL:<reason>; exits 1 on FAIL so callers can gate on it.
#   The caller computes release_exists (e.g. gh api .../releases/tags/$tag)
#   and tag_sha (git rev-parse "$tag^{}"); this function is side-effect free.
immutability_decision() {
	local event_name="$1" tag="$2" sha="$3" release_exists="$4" tag_sha="$5"

	if [[ "$release_exists" != "true" ]]; then
		echo "ALLOW"
		return 0
	fi
	if [[ "$event_name" == "workflow_dispatch" && "$tag_sha" == "$sha" ]]; then
		echo "ALLOW"
		return 0
	fi
	echo "FAIL:${RS_IMMUTABILITY_FAIL_REASON}"
	return 1
}

# --- Self-test -----------------------------------------------------------------

# Parses `resolve_release_tag` output into globals:
# RS_TEST_VERSION, RS_TEST_PRERELEASE, RS_TEST_MAJOR_MINOR, RS_TEST_MAJOR,
# RS_TEST_NAME, RS_TEST_TAGS (array).
rs_parse_output() {
	local line in_tags=0 key val
	RS_TEST_VERSION=""
	RS_TEST_PRERELEASE=""
	RS_TEST_MAJOR_MINOR=""
	RS_TEST_MAJOR=""
	RS_TEST_NAME=""
	RS_TEST_TAGS=()
	while IFS= read -r line; do
		if [[ "$in_tags" == "1" ]]; then
			if [[ "$line" == "RS_EOF" ]]; then
				in_tags=0
				continue
			fi
			RS_TEST_TAGS+=("$line")
			continue
		fi
		if [[ "$line" == "docker_tags<<RS_EOF" ]]; then
			in_tags=1
			continue
		fi
		key="${line%%=*}"
		val="${line#*=}"
		case "$key" in
			version) RS_TEST_VERSION="$val" ;;
			is_prerelease) RS_TEST_PRERELEASE="$val" ;;
			version_major_minor) RS_TEST_MAJOR_MINOR="$val" ;;
			version_major) RS_TEST_MAJOR="$val" ;;
			release_name) RS_TEST_NAME="$val" ;;
			*) ;;
		esac
	done <<< "$1"
}

RS_CHECKS=0
RS_FAILURES=0

rs_pass() { echo "PASS: $1"; }
rs_fail() {
	echo "FAIL: $1" >&2
	RS_FAILURES=$((RS_FAILURES + 1))
}

# check_tag <tag> <expected_is_prerelease> <expected_release_name> <expected_tags...>
rs_check_tag() {
	local tag="$1" expected_prerelease="$2" expected_name="$3"
	shift 3
	local output expected expected_tag

	RS_CHECKS=$((RS_CHECKS + 1))
	if ! output="$(resolve_release_tag "$tag" 2>/dev/null)"; then
		rs_fail "$tag: unexpectedly rejected"
		return
	fi
	rs_parse_output "$output"

	[[ "$RS_TEST_PRERELEASE" == "$expected_prerelease" ]] ||
		rs_fail "$tag: is_prerelease=$RS_TEST_PRERELEASE (expected $expected_prerelease)"
	[[ "$RS_TEST_NAME" == "$expected_name" ]] ||
		rs_fail "$tag: release_name='$RS_TEST_NAME' (expected '$expected_name')"

	for expected in "$@"; do
		found=false
		for expected_tag in "${RS_TEST_TAGS[@]}"; do
			[[ "$expected_tag" == "type=raw,value=$expected" ]] && found=true
		done
		[[ "$found" == "true" ]] ||
			rs_fail "$tag: docker_tags missing 'type=raw,value=$expected'"
	done

	if [[ "$expected_prerelease" == "false" ]]; then
		found=false
		for expected_tag in "${RS_TEST_TAGS[@]}"; do
			[[ "$expected_tag" == "type=raw,value=latest" ]] && found=true
		done
		[[ "$found" == "true" ]] ||
			rs_fail "$tag: stable docker_tags missing 'latest'"
		[[ "${#RS_TEST_TAGS[@]}" == "4" ]] ||
			rs_fail "$tag: stable docker_tags has ${#RS_TEST_TAGS[@]} entries (expected 4)"
	else
		found=false
		for expected_tag in "${RS_TEST_TAGS[@]}"; do
			[[ "$expected_tag" == "type=raw,value=latest" ]] && found=true
		done
		[[ "$found" == "false" ]] ||
			rs_fail "$tag: prerelease docker_tags must NOT contain 'latest'"
		[[ "${#RS_TEST_TAGS[@]}" == "1" ]] ||
			rs_fail "$tag: prerelease docker_tags has ${#RS_TEST_TAGS[@]} entries (expected 1)"
	fi
}

# check_immutability <event_name> <tag> <sha> <release_exists> <tag_sha> <expected>
rs_check_immutability() {
	local event_name="$1" tag="$2" sha="$3" release_exists="$4" tag_sha="$5" expected="$6"
	local decision

	RS_CHECKS=$((RS_CHECKS + 1))
	decision="$(immutability_decision "$event_name" "$tag" "$sha" "$release_exists" "$tag_sha" || true)"
	if [[ "$decision" == "$expected" ]]; then
		rs_pass "immutability event=$event_name tag=$tag release_exists=$release_exists tag_sha=${tag_sha:-none} -> $decision"
	else
		rs_fail "immutability event=$event_name tag=$tag release_exists=$release_exists tag_sha=${tag_sha:-none}: got '$decision' (expected '$expected')"
	fi
}

# check_rejected <tag>
rs_check_rejected() {
	local tag="$1"
	RS_CHECKS=$((RS_CHECKS + 1))
	if output="$(resolve_release_tag "$tag" 2>/dev/null)"; then
		rs_fail "$tag: expected rejection but resolved: $(echo "$output" | head -n1)"
	else
		rs_pass "rejects $tag"
	fi
}

selftest() {
	echo "release-tag.sh selftest"
	echo "---"

	# Stable: full Docker tag set (version, major_minor, major, latest), RustShare name.
	rs_check_tag "v0.7.0" false "RustShare v0.7.0" 0.7.0 0.7 0 latest
	# Build metadata is allowed and does NOT flip the prerelease flag.
	rs_check_tag "v1.2.3+meta" false "RustShare v1.2.3+meta" 1.2.3 1.2 1 latest

	# Prerelease: version-only Docker tag (no latest), Elembra name.
	rs_check_tag "v0.8.0-alpha.1" true "Elembra v0.8.0-alpha.1" 0.8.0-alpha.1
	rs_check_tag "v1.2.3-rc.2" true "Elembra v1.2.3-rc.2" 1.2.3-rc.2
	rs_check_tag "v1.2.3-beta" true "Elembra v1.2.3-beta" 1.2.3-beta
	# Uppercase prerelease identifiers are valid SemVer; build metadata is
	# accepted but stripped from the Docker tag (`+` is not a valid Docker tag char).
	rs_check_tag "v1.2.3-ALPHA" true "Elembra v1.2.3-ALPHA" 1.2.3-ALPHA
	rs_check_tag "v1.2.3-alpha.1+meta" true "Elembra v1.2.3-alpha.1+meta" 1.2.3-alpha.1

	# Release immutability decision matrix.
	# No existing release -> ALLOW, regardless of event.
	rs_check_immutability "push" "v0.7.0" "aaa1111" "false" "" "ALLOW"
	# Existing release + workflow_dispatch + same commit -> ALLOW (repair mode).
	rs_check_immutability "workflow_dispatch" "v0.8.0-alpha.1" "aaa1111" "true" "aaa1111" "ALLOW"
	# Existing release + workflow_dispatch + different commit -> FAIL.
	rs_check_immutability "workflow_dispatch" "v0.8.0-alpha.1" "aaa1111" "true" "bbb2222" "FAIL:${RS_IMMUTABILITY_FAIL_REASON}"
	# Existing release + tag push -> FAIL even when the tag points at the same
	# commit (a force-move or duplicate attempt is never allowed via push).
	rs_check_immutability "push" "v0.7.0" "aaa1111" "true" "aaa1111" "FAIL:${RS_IMMUTABILITY_FAIL_REASON}"

	# Invalid tags.
	rs_check_rejected ""
	rs_check_rejected "v"
	rs_check_rejected "v1.2"
	rs_check_rejected "v1.2.3.4"
	rs_check_rejected "v1.2.3-"
	rs_check_rejected "abc"
	rs_check_rejected "1.2.3"
	rs_check_rejected "v01.2.3"
	rs_check_rejected "v1.02.3"
	rs_check_rejected "v1.2.3-01"

	echo "---"
	echo "$RS_CHECKS checks, $RS_FAILURES failures"
	[[ "$RS_FAILURES" == "0" ]]
}

if [[ "${1:-}" == "--selftest" ]]; then
	selftest
fi
