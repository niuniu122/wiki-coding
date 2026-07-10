import React from "react";
import {Text, useInput} from "ink";

interface TextInputProps {
  value: string;
  onChange(value: string): void;
  onSubmit(value: string): void;
  placeholder?: string;
  mask?: boolean;
  disabled?: boolean;
}

export function TextInput({
  value,
  onChange,
  onSubmit,
  placeholder,
  mask = false,
  disabled = false
}: TextInputProps): React.ReactElement {
  useInput((input, key) => {
    if (disabled) {
      return;
    }

    if (key.return) {
      onSubmit(value);
      return;
    }

    if (key.backspace || key.delete) {
      onChange(value.slice(0, -1));
      return;
    }

    if (key.ctrl && input === "u") {
      onChange("");
      return;
    }

    if (input) {
      onChange(`${value}${input}`);
    }
  });

  const visible = mask ? "*".repeat(value.length) : value;
  const shown = visible || placeholder || "";

  return (
    <Text>
      <Text color="green">{"> "}</Text>
      <Text color={value ? "white" : "gray"}>{shown}</Text>
      {!disabled && <Text color="green">_</Text>}
    </Text>
  );
}
