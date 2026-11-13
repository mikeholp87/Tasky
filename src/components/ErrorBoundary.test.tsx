import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

let broken = true;
function Flaky() {
  if (broken) throw new Error("boom");
  return <p>Page content</p>;
}

beforeEach(() => {
  broken = true;
  // React and the boundary both log the caught error.
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

describe("ErrorBoundary", () => {
  it("shows the error instead of a blank page and reloads the page on request", async () => {
    render(
      <>
        <nav>Sidebar</nav>
        <ErrorBoundary>
          <Flaky />
        </ErrorBoundary>
      </>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("boom");
    expect(screen.getByText("Sidebar")).toBeInTheDocument();

    broken = false;
    await userEvent.click(screen.getByRole("button", { name: "Reload page" }));
    expect(screen.getByText("Page content")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("starts fresh when its key changes (another page)", () => {
    const { rerender } = render(
      <ErrorBoundary key="a">
        <Flaky />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    broken = false;
    rerender(
      <ErrorBoundary key="b">
        <Flaky />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Page content")).toBeInTheDocument();
  });
});
