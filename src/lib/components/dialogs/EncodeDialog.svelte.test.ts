import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";

import EncodeDialog from "./EncodeDialog.svelte";

function renderDialog(onSubmit = vi.fn()) {
  render(EncodeDialog, {
    props: {
      inputPath: "/media/movie.mp4",
      initialOutputPath: "/media/movie.x265.mkv",
      onSubmit,
      onCancel: vi.fn(),
    },
  });
  return onSubmit;
}

describe("EncodeDialog", () => {
  it("submits clip times, output, and subtitle preference", async () => {
    const onSubmit = renderDialog();
    const inputs = screen.getAllByRole("textbox");
    await fireEvent.input(inputs[0], { target: { value: "00:01:00" } });
    await fireEvent.input(inputs[1], { target: { value: "00:03:30.5" } });
    await fireEvent.input(inputs[2], { target: { value: "/exports/clip.mkv" } });
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Encode" }));

    expect(onSubmit).toHaveBeenCalledWith({
      outputPath: "/exports/clip.mkv",
      startTime: "00:01:00",
      endTime: "00:03:30.5",
      burnSubtitles: true,
    });
  });

  it("rejects an end time before the start", async () => {
    const onSubmit = renderDialog();
    const inputs = screen.getAllByRole("textbox");
    await fireEvent.input(inputs[0], { target: { value: "20" } });
    await fireEvent.input(inputs[1], { target: { value: "10" } });
    await fireEvent.click(screen.getByRole("button", { name: "Encode" }));

    expect(screen.getByRole("alert")).toHaveTextContent("after the start");
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
