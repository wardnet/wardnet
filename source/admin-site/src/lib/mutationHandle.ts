/**
 * The narrow slice of a TanStack `useMutation` result that a presentation
 * card receives as a prop once the owning page hoists the mutation (the
 * component-layer rule in `.agents/code-conventions.md`). A page passes the
 * mutation result straight through — `UseMutationResult` is structurally
 * assignable to this — while tests can satisfy it with plain `vi.fn()`s
 * instead of stubbing whole hook shapes.
 */
export interface MutationHandle<TVariables> {
  mutateAsync: (variables: TVariables) => Promise<unknown>;
  reset: () => void;
  isPending: boolean;
  isError: boolean;
  error: Error | null;
}
