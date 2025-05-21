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
        className="flex w-6 h-6 appearance-none items-center justify-center rounded border-neutral-600 border outline-none hover:bg-slate-700  focus:outline-1 focus:outline-white transition-colors"
        checked={props.checked}
        onCheckedChange={props.onCheckedChange}
      >
        <Checkbox.Indicator className="text-white bg-sky-600 rounded border-neutral-600 border">
          <CheckIcon />
        </Checkbox.Indicator>
      </Checkbox.Root>
      <label className="pl-3 leading-none text-white">{props.label}</label>
    </div>
  </form>
);

export { StyledCheckbox };
export type { CheckedState };
