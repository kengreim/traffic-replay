import { Checkbox } from "radix-ui";
import { CheckIcon } from "lucide-react";

interface CheckboxProps {
  label?: string;
  checked?: Checkbox.CheckedState | undefined;
  onCheckedChange?: (checked: Checkbox.CheckedState) => void;
}

type CheckedState = Checkbox.CheckedState;

const StyledCheckbox = (props: CheckboxProps) => (
  <form>
    <div className="flex items-center">
      <Checkbox.Root
        className="flex h-6 w-6 appearance-none items-center justify-center rounded border border-neutral-600 outline-none transition-colors hover:bg-slate-700 focus:outline-1 focus:outline-white"
        checked={props.checked}
        onCheckedChange={props.onCheckedChange}
      >
        <Checkbox.Indicator className="rounded border border-neutral-600 bg-sky-600 text-white">
          <CheckIcon />
        </Checkbox.Indicator>
      </Checkbox.Root>
      <label className="pl-3 leading-none text-white">{props.label}</label>
    </div>
  </form>
);

export { StyledCheckbox };
export type { CheckedState };
