import { readable, type Readable } from 'svelte/store';
import {
	QueryClientProvider,
	QueryClient
} from '@tanstack/svelte-query';
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

type QueryStoreResult<
	TQueryFnData,
	TError,
	TData,
	TQueryData,
	TQueryKey extends QueryKey
> = Readable<QueryObserverResult<TData, TError>> & {
	refetch: QueryObserver<TQueryFnData, TError, TData, TQueryData, TQueryKey>['refetch'];
	remove: QueryObserver<TQueryFnData, TError, TData, TQueryData, TQueryKey>['remove'];
};

type MutationStoreResult<TData, TError, TVariables, TContext> =
	Readable<MutationObserverResult<TData, TError, TVariables, TContext>> & {
		mutate: (variables: TVariables, options?: MutateOptions<TData, TError, TVariables, TContext>) => void;
		mutateAsync: (variables: TVariables, options?: MutateOptions<TData, TError, TVariables, TContext>) => Promise<TData>;
		reset: () => void;
	};

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

	const makeResult = () =>
		observer.getOptimisticResult(queryClient.defaultQueryOptions(options));

	const store = readable<QueryObserverResult<TData, TError>>(makeResult(), (set) => {
		observer.setOptions(options);
		set(makeResult());
		return observer.subscribe((result) => {
			set(result);
		});
	});

	return {
		subscribe: store.subscribe,
		refetch: observer.refetch.bind(observer),
		remove: observer.remove.bind(observer)
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

	const makeResult = () => observer.getCurrentResult();

	const store = readable<MutationObserverResult<TData, TError, TVariables, TContext>>(
		makeResult(),
		(set) => {
			observer.setOptions(options);
			set(makeResult());
			return observer.subscribe((result) => {
				set(result);
			});
		}
	);

	return {
		subscribe: store.subscribe,
		mutate: (variables, mutateOptions) => {
			void observer.mutate(variables, mutateOptions).catch(() => {
				// Keep parity with store-style mutate(), which exposes state through the store.
			});
		},
		mutateAsync: (variables, mutateOptions) => observer.mutate(variables, mutateOptions),
		reset: () => observer.reset()
	};
}
