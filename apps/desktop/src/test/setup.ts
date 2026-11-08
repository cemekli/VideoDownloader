import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Vitest runs without globals, so Testing Library cannot unmount on its own.
afterEach(() => {
  cleanup();
  try {
    localStorage.clear();
  } catch {
    // no storage in this environment
  }
});
