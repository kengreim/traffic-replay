import { Pause, Play, StepBack, StepForward } from "lucide-react";
import { Slider } from "radix-ui";

export function PlaybackBar() {
  return (
    <div className="absolute bottom-5 w-full p-5">
      {Object.keys(trafficData).length > 0 && (
        <div className="flex items-end rounded bg-neutral-50/90 px-5 py-2 shadow">
          <div className="flex flex-col items-center">
            <div className="flex">
              <StepBack
                className="cursor-pointer hover:scale-105"
                onClick={() => {
                  setIsPlaying(false);
                  decrementTimeSlider();
                }}
              />
              {isPlaying ? (
                <Pause className="cursor-pointer hover:scale-105" onClick={togglePlayback} />
              ) : (
                <Play className="cursor-pointer hover:scale-105" onClick={togglePlayback} />
              )}
              <StepForward
                className="cursor-pointer hover:scale-105"
                onClick={() => {
                  setIsPlaying(false);
                  incrementTimeSlider();
                }}
              />
            </div>
            <div className="mt-2">
              <select
                id="playback-speed"
                value={playbackSpeed}
                onChange={(e) => {
                  setPlaybackSpeed(Number(e.target.value));
                  if (isPlaying) {
                    setIsPlaying(false);
                    setTimeout(() => setIsPlaying(true), 0);
                  }
                }}
                className="rounded border border-gray-300 px-2 py-1 text-sm"
              >
                <option value={1}>1x</option>
                <option value={2}>2x</option>
                <option value={4}>4x</option>
                <option value={8}>8x</option>
                <option value={16}>16x</option>
              </select>
            </div>
          </div>
          <div className="mb-5 ml-6 grow">
            <form className="px-4">
              <Slider.Root
                className="relative flex h-5 w-full touch-none select-none items-center pt-7"
                defaultValue={[0]}
                min={0}
                max={timestamps.length - 1}
                step={1}
                value={[sliderIndex]}
                onValueChange={(v) => setSliderIndex(v[0])}
                onPointerDown={() => {
                  if (isPlaying) {
                    setIsPlaying(false);
                    pointerDownSuspendedPlay.current = true;
                  }
                }}
                onPointerUp={() => {
                  if (pointerDownSuspendedPlay.current) {
                    setIsPlaying(true);
                    pointerDownSuspendedPlay.current = false;
                  }
                }}
              >
                <Slider.Track className="relative h-[3px] grow rounded-full bg-neutral-300">
                  <Slider.Range className="absolute h-full rounded-full bg-slate-700" />
                </Slider.Track>
                <Slider.Thumb
                  className="block size-5 rounded-[10px] bg-white shadow-[0_2px_10px] shadow-black transition-colors hover:bg-sky-600 focus:bg-sky-600 focus:shadow-[0_0_0_5px] focus:shadow-black focus:outline-none"
                  aria-label="Volume"
                >
                  <div className="relative -left-6 -top-8 font-mono">{timestampString}</div>
                </Slider.Thumb>
              </Slider.Root>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
