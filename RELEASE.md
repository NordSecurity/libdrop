# Releases
There are currently these branches being used for releases.
- release/v5
- release/v6

# Branches
As of 6.4 and 5.5 versions libdrop needs to maintain two versions of Moose tracker. One with
context sharing and the other with not. Both are not backwards compatible, thus Libdrop 
must maintain two release branches for a while.

branches and releases must contain `moose-no-context` or `moose-with-context` in the name to
indicate which moose tracker is being used. *This should be a temporary nuisance, a constant re-evaluation is needed*.

# Release procedure

1. Check the `changelog.md` file and make appropriate missing changes if any. Enter new version in there at the top if not there.
Preferrably there should be no version before the release happens because it's easy to assume it's an already released version.
Prefer to have *UNRELEASED* as a placeholder.

2. Tag the commit with changelog modifications.
3. Enjoy as Gitlab pipeline is triggered and produces builds for various platforms.

# Tips
- if `changelog.md` was forgotten to be updated before tagging there are two solutions:
    - do changelog modifications and push them. Then either:
        - Do not modify the existing tags or releases. *This, while easier, is not preferred as it makes the tags disconnected from changelog entries*.
        - Delete the tag, re-create the release tag and proceed as usual. *This will require some manual intervention as build artifacts will be already stored*.