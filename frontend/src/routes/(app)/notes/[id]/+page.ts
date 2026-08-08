import type { PageLoad } from './$types';
import { redirect } from '@sveltejs/kit';

export const load: PageLoad = async ({ params }) => {
	throw redirect(308, `/apps/notes/${params.id}`);
};
