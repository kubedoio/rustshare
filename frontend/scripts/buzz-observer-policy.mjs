export function isUnboundAuthorRejection(status, body) {
	return status === 403 && body?.error === 'Unbound author';
}
