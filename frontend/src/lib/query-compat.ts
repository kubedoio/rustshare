import { type Readable } from 'svelte/store';
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

function queryResultsEqual<TData, TError>(
	a: QueryObserverResult<TData, TError>,
	b: QueryObserverResult<TData, TError>
): boolean {
	return (
		a === b ||
		(a.data === b.data &&
			a.error === b.error &&
			a.status === b.status &&
			a.isLoading === b.isLoading &&
			a.isFetching === b.isFetching &&
			a.isSuccess === b.isSuccess &&
			a.isError === b.isError &&
			a.fetchStatus === b.fetchStatus)
	);
}

function mutationResultsEqual<TData, TError, TVariables, TContext>(
	a: MutationObserverResult<TData, TError, TVariables, TContext>,
	b: MutationObserverResult<TData, TError, TVariables, TContext>
): boolean {
	return (
		a === b ||
		(a.data === b.data &&
			a.error === b.error &&
			a.status === b.status &&
			a.isIdle === b.isIdle &&
			a.isPending === b.isPending &&
			a.isSuccess === b.isSuccess &&
			a.isError === b.isError)
	);
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
	let prevResult: QueryObserverResult<TData, TError> | undefined;

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

	function emitResult(
		result: QueryObserverResult<TData, TError>,
		run: (value: QueryStoreValue<TQueryFnData, TError, TData, TQueryData, TQueryKey>) => void
	) {
		if (prevResult && queryResultsEqual(prevResult, result)) {
			return;
		}
		prevResult = result;
		run({ ...result, ...methods });
	}

	return {
		subscribe: (run) => {
			const initialResult = observer.getOptimisticResult(
				queryClient.defaultQueryOptions(currentOptions)
			);
			emitResult(initialResult, run);
			return observer.subscribe((result) => {
				emitResult(result, run);
			});
		},
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
	let prevResult: MutationObserverResult<TData, TError, TVariables, TContext> | undefined;

	const methods: MutationMethods<TData, TError, TVariables, TContext> = {
		mutate: (variables, mutateOptions) => observer.mutate(variables, mutateOptions),
		mutateAsync: (variables, mutateOptions) => observer.mutate(variables, mutateOptions),
		reset: () => observer.reset()
	};

	function emitResult(
		result: MutationObserverResult<TData, TError, TVariables, TContext>,
		run: (value: MutationStoreValue<TData, TError, TVariables, TContext>) => void
	) {
		if (prevResult && mutationResultsEqual(prevResult, result)) {
			return;
		}
		prevResult = result;
		run({ ...result, ...methods });
	}

	return {
		subscribe: (run) => {
			const initialResult = observer.getCurrentResult();
			emitResult(initialResult, run);
			return observer.subscribe((result) => {
				emitResult(result, run);
			});
		},
		...methods
	};
}
