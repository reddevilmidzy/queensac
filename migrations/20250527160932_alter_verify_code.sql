ALTER TABLE subscribers
ADD COLUMN IF NOT EXISTS verification_code VARCHAR(255),
ADD COLUMN IF NOT EXISTS verification_code_created_at TIMESTAMP WITH TIME ZONE,
ADD COLUMN IF NOT EXISTS verification_code_expires_at TIMESTAMP WITH TIME ZONE,
ADD COLUMN IF NOT EXISTS is_verified BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_subscribers_verification_expires 
ON subscribers(verification_code_expires_at);
