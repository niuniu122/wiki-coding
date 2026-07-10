export class ReasoningFilter {
  hiddenCharacters = 0;
  #pending = "";
  #inThink = false;
  #trimNextVisible = false;

  process(chunk: string): string[] {
    this.#pending += chunk;
    return this.drain(false);
  }

  hide(content: string): void {
    this.hiddenCharacters += content.length;
  }

  flush(): string[] {
    return this.drain(true);
  }

  private drain(final: boolean): string[] {
    const visible: string[] = [];

    while (this.#pending.length > 0) {
      if (this.#inThink) {
        const endIndex = findTag(this.#pending, "</think>");
        if (endIndex === -1) {
          const hiddenLength = final
            ? this.#pending.length
            : safeEmitLength(this.#pending, "</think>");
          if (hiddenLength > 0) {
            this.hiddenCharacters += hiddenLength;
            this.#pending = this.#pending.slice(hiddenLength);
          }
          break;
        }

        this.hiddenCharacters += endIndex;
        this.#pending = this.#pending.slice(endIndex + "</think>".length);
        this.#inThink = false;
        this.#trimNextVisible = true;
        continue;
      }

      const startIndex = findTag(this.#pending, "<think>");
      if (startIndex === -1) {
        const visibleLength = final
          ? this.#pending.length
          : safeEmitLength(this.#pending, "<think>");
        if (visibleLength > 0) {
          this.pushVisible(visible, this.#pending.slice(0, visibleLength));
          this.#pending = this.#pending.slice(visibleLength);
        }
        break;
      }

      if (startIndex > 0) {
        this.pushVisible(visible, this.#pending.slice(0, startIndex));
      }
      this.#pending = this.#pending.slice(startIndex + "<think>".length);
      this.#inThink = true;
    }

    return visible;
  }

  private pushVisible(output: string[], text: string): void {
    const visible = this.#trimNextVisible ? text.replace(/^\s+/, "") : text;
    if (visible.length > 0) {
      output.push(visible);
      this.#trimNextVisible = false;
    }
  }
}

function findTag(text: string, tag: "<think>" | "</think>"): number {
  return text.toLowerCase().indexOf(tag);
}

function safeEmitLength(text: string, tag: "<think>" | "</think>"): number {
  const lowerText = text.toLowerCase();
  const max = Math.min(tag.length - 1, lowerText.length);
  for (let length = max; length > 0; length--) {
    if (tag.startsWith(lowerText.slice(-length))) {
      return text.length - length;
    }
  }
  return text.length;
}
