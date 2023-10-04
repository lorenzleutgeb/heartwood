--
-- Session storage SQL schema.
--
CREATE TABLE IF NOT EXISTS "sessions"
(
  -- Session ID
  "id"         TEXT PRIMARY KEY NOT NULL,
  -- Session status, either unauthorized or authorized
  "status"     TEXT             NOT NULL DEFAULT 'unauthorized',
  -- The public key for which the session was issued
  "public_key" TEXT             NOT NULL,
  -- Node alias.
  "alias"      TEXT             NOT NULL,
  --- Session creation time
  "issued_at"  INTEGER          NOT NULL,
  -- Session expiration timestamp. If <=0, the session never expires
  "expires_at" INTEGER          NOT NULL,
  -- A comment explaining what the session is about,
  -- useful if session is used as api key with increased expiration
  "comment"    TEXT
  --
) STRICT;
