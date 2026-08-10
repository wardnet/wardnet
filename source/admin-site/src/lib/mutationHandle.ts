/**
 * The narrow slice of a TanStack `useMutation` result that a presentation
 * card receives as a prop once the owning page hoists the mutation (the
 * component-layer rule in `.agents/code-conventions.md`). A page passes the
 * mutation result straight through — `UseMutationResult` is structurally
 * assignable to this — while tests can satisfy it with plain `vi.fn()`s
 * instead of stubbing whole hook shapes. `TData` types the resolved value
 * for cards that consume it (e.g. a dry-run preview's response); leave it
 * defaulted when the card only awaits completion.
 */
export interface MutationHandle<TVariables, TData = unknown> {
  mutateAsync: (variables: TVariables) => Promise<TData>;
  reset: () => void;
  isPending: boolean;
  isError: boolean;
  error: Error | null;
}

/**
 * A hoisted mutation's fire-and-forget entry point. Matches TanStack's
 * `mutate` signature so the page can pass `mutation.mutate` straight
 * through; cards use the optional `onSuccess` to close inline forms on a
 * successful save.
 */
export type MutateFn<TVariables> = (
  variables: TVariables,
  callbacks?: { onSuccess?: () => void },
) => void;
