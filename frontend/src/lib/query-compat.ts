import { readable, type Readable } from 'svelte/store';
import { QueryClientProvider, QueryClient } from '@tanstack/svelte-query';
import {
	QueryObserver,
	MutationObserver,
	type QueryKey,
	type QueryObserverOptions,
	type QueryObserverResult,
	type MutationObserverOptions,
	type MutationObserverResult,
	type MutateOptions
} from '@tanstack/query-core';
import { queryClient } from '$lib/query-client';

type QueryMethods<TQueryFnData, TError, TData, TQueryData, TQueryKey extends QueryKey> = {
	refetch: QueryObserver<TQueryFnData, TError, TData, TQueryData, TQueryKey>['refetch'];
	remove: () => void;
	setOptions: (
		options: QueryObserverOptions<TQueryFnData, TError, TData, TQueryData, TQueryKey>
	) => void;
};

type MutationMethods<TData, TError, TVariables, TContext> = {
	mutate: (
		variables: TVariables,
		options?: MutateOptions<TData, TError, TVariables, TContext>
	) => Promise<TData>;
	mutateAsync: (
		variables: TVariables,
		options?: MutateOptions<TData, TError, TVariables, TContext>
	) => Promise<TData>;
	reset: () => void;
};

type QueryStoreValue<
	TQueryFnData,
	TError,
	TData,
	TQueryData,
	TQueryKey extends QueryKey
> = QueryObserverResult<TData, TError> &
	QueryMethods<TQueryFnData, TError, TData, TQueryData, TQueryKey>;

type MutationStoreValue<TData, TError, TVariables, TContext> = MutationObserverResult<
	TData,
	TError,
	TVariables,
	TContext
> &
	MutationMethods<TData, TError, TVariables, TContext>;

type QueryStoreResult<
	TQueryFnData,
	TError,
	TData,
	TQueryData,
	TQueryKey extends QueryKey
> = Readable<QueryStoreValue<TQueryFnData, TError, TData, TQueryData, TQueryKey>> &
	QueryMethods<TQueryFnData, TError, TData, TQueryData, TQueryKey>;

type MutationStoreResult<TData, TError, TVariables, TContext> = Readable<
	MutationStoreValue<TData, TError, TVariables, TContext>
> &
	MutationMethods<TData, TError, TVariables, TContext>;

export { QueryClientProvider, QueryClient };

export function useQueryClient() {
	return queryClient;
}

export function createQuery<
	TQueryFnData = unknown,
	TError = Error,
	TData = TQueryFnData,
	TQueryData = TQueryFnData,
	TQueryKey extends QueryKey = QueryKey
>(
	options: QueryObserverOptions<TQueryFnData, TError, TData, TQueryData, TQueryKey>
): QueryStoreResult<TQueryFnData, TError, TData, TQueryData, TQueryKey> {
	const observer = new QueryObserver<TQueryFnData, TError, TData, TQueryData, TQueryKey>(
		queryClient,
		options
	);

	let currentOptions = options;

	const methods: QueryMethods<TQueryFnData, TError, TData, TQueryData, TQueryKey> = {
		refetch: observer.refetch.bind(observer),
		remove: () => {
			if (currentOptions.queryKey) {
				queryClient.removeQueries({ queryKey: currentOptions.queryKey });
			}
		},
		setOptions: (newOptions) => {
			currentOptions = newOptions;
			observer.setOptions(newOptions);
		}
	};

	const makeResult = (): QueryStoreValue<TQueryFnData, TError, TData, TQueryData, TQueryKey> => ({
		...observer.getOptimisticResult(queryClient.defaultQueryOptions(options)),
		...methods
	});

	const store = readable<QueryStoreValue<TQueryFnData, TError, TData, TQueryData, TQueryKey>>(
		makeResult(),
		(set) => {
			observer.setOptions(options);
			set(makeResult());
			return observer.subscribe((result) => {
				set({ ...result, ...methods });
			});
		}
	);

	return {
		subscribe: store.subscribe,
		...methods
	};
}

export function createMutation<
	TData = unknown,
	TError = Error,
	TVariables = void,
	TContext = unknown
>(
	options: MutationObserverOptions<TData, TError, TVariables, TContext>
): MutationStoreResult<TData, TError, TVariables, TContext> {
	const observer = new MutationObserver<TData, TError, TVariables, TContext>(queryClient, options);
	const methods: MutationMethods<TData, TError, TVariables, TContext> = {
		mutate: (variables, mutateOptions) => observer.mutate(variables, mutateOptions),
		mutateAsync: (variables, mutateOptions) => observer.mutate(variables, mutateOptions),
		reset: () => observer.reset()
	};

	const makeResult = (): MutationStoreValue<TData, TError, TVariables, TContext> => ({
		...observer.getCurrentResult(),
		...methods
	});

	const store = readable<MutationStoreValue<TData, TError, TVariables, TContext>>(
		makeResult(),
		(set) => {
			observer.setOptions(options);
			set(makeResult());
			return observer.subscribe((result) => {
				set({ ...result, ...methods });
			});
		}
	);

	return {
		subscribe: store.subscribe,
		...methods
	};
}
