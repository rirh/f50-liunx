import { type PropsWithChildren, useState } from "react";
import { useSession } from "@/hooks/use-session";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

function SessionQueries({ children }: PropsWithChildren) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: 1,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );

  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

export function QueryProvider({ children }: PropsWithChildren) {
  const session = useSession();
  return (
    <SessionQueries key={session?.id ?? "signed-out"}>
      {children}
    </SessionQueries>
  );
}
