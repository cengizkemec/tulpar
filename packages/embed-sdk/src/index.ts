export type EmbedWidget =
  | "community"
  | "join"
  | "members"
  | "voice_preview";

export interface EmbedMountOptions {
  /** Public `emb_...` key from an embed application. */
  key: string;
  /** Tulpar instance origin. Defaults to the origin that served the SDK loader. */
  instance?: string;
  widget?: EmbedWidget;
  height?: number;
  title?: string;
  /** Request an authenticated member view. The widget shows a sign-in button. */
  authenticated?: boolean;
}

export interface EmbedHandle {
  iframe: HTMLIFrameElement;
  destroy: () => void;
}

interface TulparEmbedGlobal {
  mount: (element: HTMLElement, options: EmbedMountOptions) => EmbedHandle;
}

declare global {
  interface Window {
    TulparEmbed?: TulparEmbedGlobal;
  }
}

/**
 * Mount a Tulpar embed into an element. The instance SDK loader must be present:
 *
 * ```html
 * <script src="https://tulpar.example.com/embed/v1/tulpar-embed.js"></script>
 * ```
 */
export function mount(
  element: HTMLElement,
  options: EmbedMountOptions,
): EmbedHandle {
  if (!window.TulparEmbed) {
    throw new Error(
      "Tulpar embed loader is missing. Include /embed/v1/tulpar-embed.js first.",
    );
  }
  return window.TulparEmbed.mount(element, options);
}
