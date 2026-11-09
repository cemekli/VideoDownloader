-- 0012: authorized session profiles (SPEC §11.6 `AuthProfile`, §12.2,
-- §20.3; STATE.md F5.1; ADR-0022).
--
-- No secret value is ever stored here: `secure_local_ref` is an opaque
-- reference into the OS-protected secret store (vv-secrets), and the
-- browser method keeps only which browser and profile to read at job time.
CREATE TABLE auth_profile (
    id               TEXT PRIMARY KEY NOT NULL,
    -- One platform per profile; consent is never for "any site".
    provider         TEXT NOT NULL CHECK (provider IN (
                         'youtube', 'facebook', 'instagram', 'tiktok', 'x', 'vk',
                         'dailymotion', 'twitch', 'reddit')),
    label            TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 200),
    -- browser_cookies: yt-dlp reads the browser's store at job time (F5.2).
    -- cookie_file: a user-exported file, filtered to the platform (F5.3).
    -- app_profile: a browser profile owned by the app (F5.7 research).
    method           TEXT NOT NULL CHECK (method IN ('browser_cookies', 'cookie_file', 'app_profile')),
    browser_kind     TEXT CHECK (browser_kind IS NULL OR browser_kind IN (
                         'firefox', 'chrome', 'edge', 'brave', 'chromium', 'opera', 'vivaldi')),
    -- The browser's profile name or folder; a private path, never logged.
    profile_locator  TEXT CHECK (profile_locator IS NULL OR length(profile_locator) BETWEEN 1 AND 1024),
    secure_local_ref TEXT UNIQUE CHECK (secure_local_ref IS NULL OR (
                         length(secure_local_ref) = 42 AND substr(secure_local_ref, 1, 6) = 'vvsec:')),
    session_state    TEXT NOT NULL DEFAULT 'needs_verification' CHECK (session_state IN (
                         'not_connected', 'needs_verification', 'session_available', 'expired', 'unsupported')),
    -- Operations the user allowed, e.g. ["metadata","download","verify"].
    consent_scope    TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(consent_scope) AND json_type(consent_scope) = 'array'),
    consent_at       INTEGER,
    last_verified_at INTEGER,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    CHECK ((method = 'browser_cookies') = (browser_kind IS NOT NULL)),
    -- Consent and its scope come and go together.
    CHECK ((consent_at IS NULL) = (consent_scope = '[]')),
    -- "Session available" needs a verification time (§12.1 evidence rule).
    CHECK (session_state <> 'session_available' OR last_verified_at IS NOT NULL)
);

CREATE INDEX auth_profile_provider ON auth_profile (provider, created_at);

-- An item's last authorized access names a profile; forget it with the
-- profile instead of leaving a dangling id.
CREATE TRIGGER auth_profile_deleted AFTER DELETE ON auth_profile
BEGIN
    UPDATE library_item SET access_profile_id = NULL WHERE access_profile_id = OLD.id;
END;
