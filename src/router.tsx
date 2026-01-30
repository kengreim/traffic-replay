import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import App from "./App";
import { EventAutoLoader } from "./components/EventAutoLoader";

const rootRoute = createRootRoute({
  component: App,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
});

const slugRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/$slug",
  component: EventAutoLoader,
});

const routeTree = rootRoute.addChildren([indexRoute, slugRoute]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
