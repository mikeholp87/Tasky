import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

type Size = "s" | "m" | "l";

function Picker({ disabled }: { disabled?: boolean }) {
  const [value, setValue] = useState<Size>("m");
  return (
    <>
      <button type="button">Before</button>
      <SegmentedControl
        label="Size"
        value={value}
        onChange={setValue}
        disabled={disabled}
        segments={[
          { value: "s", label: "Small" },
          { value: "m", label: "Medium" },
          { value: "l", label: "L", title: "Large" },
        ]}
      />
      <button type="button">After</button>
    </>
  );
}

/**
 * Radix moves focus in a timeout and selects on focus only while the arrow
 * key is still down, so release it after focus has moved, like a real press.
 */
async function press(key: string) {
  const user = userEvent.setup();
  await user.keyboard(`{${key}>}`);
  await new Promise((resolve) => setTimeout(resolve, 0));
  await user.keyboard(`{/${key}}`);
}

describe("SegmentedControl", () => {
  it("is one tab stop on the selected segment", async () => {
    render(<Picker />);
    screen.getByRole("button", { name: "Before" }).focus();
    await userEvent.tab();
    expect(screen.getByRole("radio", { name: "Medium" })).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
  });

  it("moves and selects with the arrow keys, wrapping at the ends", async () => {
    render(<Picker />);
    expect(screen.getByRole("radiogroup", { name: "Size" })).toBeInTheDocument();
    screen.getByRole("radio", { name: "Medium" }).focus();

    await press("ArrowRight");
    const large = screen.getByRole("radio", { name: "Large" });
    expect(large).toHaveFocus();
    expect(large).toBeChecked();

    await press("ArrowRight");
    expect(screen.getByRole("radio", { name: "Small" })).toBeChecked();

    await press("ArrowLeft");
    expect(screen.getByRole("radio", { name: "Large" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Small" })).not.toBeChecked();
  });

  it("selects on click and disables every segment", async () => {
    const { unmount } = render(<Picker />);
    await userEvent.click(screen.getByRole("radio", { name: "Small" }));
    expect(screen.getByRole("radio", { name: "Small" })).toBeChecked();
    unmount();

    render(<Picker disabled />);
    for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
  });
});
