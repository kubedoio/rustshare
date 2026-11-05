export const channelRef = (event) => {
	const tags = Array.isArray(event?.tags) ? event.tags.filter((tag) => Array.isArray(tag) && tag[0] === 'h') : [];
	const valid = tags.length === 1 && tags[0].length === 2 && typeof tags[0][1] === 'string';
	return { tags, valid, channel_id: valid ? tags[0][1] : null };
};
