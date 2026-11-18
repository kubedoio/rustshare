import { afterNavigate } from '$app/navigation';
import { page as currentPage } from '$app/state';

export const page = {
	subscribe(run: (value: typeof currentPage) => void) {
		run(currentPage);
		afterNavigate(() => run(currentPage));
		return () => {};
	}
};
