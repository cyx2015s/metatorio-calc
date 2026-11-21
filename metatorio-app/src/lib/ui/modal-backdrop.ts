export type BackdropCloseHandlers = {
  onpointerdown: (event: PointerEvent) => void;
  onclick: (event: MouseEvent) => void;
  onkeydown: (event: KeyboardEvent) => void;
};

/** 遮罩键盘处理：Escape 随时关闭并阻断向外层冒泡；Enter/Space 仅在焦点位于遮罩本身时关闭，防止在弹窗输入框里按空格误关弹窗。 */
export function backdropKeydown(event: KeyboardEvent, close: () => void): void {
  if (event.isComposing) return;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    close();
    return;
  }
  if ((event.key === "Enter" || event.key === " ") && event.target === event.currentTarget) {
    event.preventDefault();
    close();
  }
}

/** 生成弹窗遮罩防误关事件组：仅当主键按下起点与点击落点都在遮罩本身时才关闭，防止在弹窗内拖选文本滑出弹窗外松手导致误关。 */
export function createBackdropClose(onClose: () => void): BackdropCloseHandlers {
  let startedOnBackdrop = false;
  return {
    onpointerdown(event: PointerEvent) {
      startedOnBackdrop = event.target === event.currentTarget && event.button === 0;
    },
    onclick(event: MouseEvent) {
      const shouldClose = startedOnBackdrop && event.target === event.currentTarget;
      startedOnBackdrop = false;
      if (shouldClose) onClose();
    },
    onkeydown(event: KeyboardEvent) {
      backdropKeydown(event, onClose);
    },
  };
}
