import type { PageLoad } from './$types';
import { redirect } from '@sveltejs/kit';

export const load: PageLoad = async ({ params }) => {
	throw redirect(308, `/modules/notes/${params.id}`);
};
