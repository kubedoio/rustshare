CREATE TABLE public.memory_worker_request_signatures (
    signature_hash BYTEA PRIMARY KEY CHECK (octet_length(signature_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX memory_worker_request_signatures_expires_at_idx
    ON public.memory_worker_request_signatures (expires_at);
