from __future__ import annotations

from dataclasses import dataclass
from typing import Generic, Protocol, TypeVar

from prompt_toolkit.application import Application
from prompt_toolkit.filters import Condition
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.layout import HSplit, Layout, VSplit, Window
from prompt_toolkit.layout.containers import ConditionalContainer
from prompt_toolkit.layout.controls import FormattedTextControl
from prompt_toolkit.styles import Style
from prompt_toolkit.widgets import Frame, RadioList

MenuValue = TypeVar("MenuValue")

DEFAULT_HELP_TEXT = "Up/Down move | Enter select | Esc cancel"
DEFAULT_EMPTY_PREVIEW = "No details available."
DEFAULT_PREVIEW_TITLE = "Details"

PROMPT_STYLE = Style.from_dict(
    {
        "frame.label": "bold",
        "menu-title": "bold",
        "menu-help": "fg:#888888",
        "menu-status": "fg:ansigreen",
        "menu-preview": "",
        "menu-item-installed": "fg:ansigreen",
    }
)


@dataclass(frozen=True)
class MenuItem(Generic[MenuValue]):
    value: MenuValue
    label: str
    preview_lines: tuple[str, ...] = ()
    style: str = ""


@dataclass(frozen=True)
class Menu(Generic[MenuValue]):
    title: str
    items: tuple[MenuItem[MenuValue], ...]
    default: MenuValue | None = None
    preview_title: str = DEFAULT_PREVIEW_TITLE
    status: str | None = None
    help_text: str = DEFAULT_HELP_TEXT
    empty_preview: str = DEFAULT_EMPTY_PREVIEW


class InteractiveUi(Protocol):
    def select(self, menu: Menu[MenuValue]) -> MenuValue | None: ...

    async def select_async(self, menu: Menu[MenuValue]) -> MenuValue | None: ...


class PromptToolkitUi(InteractiveUi):
    def _build_application(
        self, menu: Menu[MenuValue]
    ) -> Application[MenuValue | None] | None:
        if not menu.items:
            return None

        def formatted_label(item: MenuItem[MenuValue]) -> str | list[tuple[str, str]]:
            if not item.style:
                return item.label
            return [(item.style, item.label)]

        radio_list = RadioList(
            [(item.value, formatted_label(item)) for item in menu.items],
            default=menu.default,
            select_on_focus=True,
            show_scrollbar=True,
        )

        def selected_item() -> MenuItem[MenuValue]:
            return menu.items[radio_list._selected_index]

        def preview_text() -> str:
            lines = selected_item().preview_lines
            return "\n".join(lines) if lines else menu.empty_preview

        key_bindings = KeyBindings()

        @key_bindings.add("enter", eager=True)
        def accept(event: object) -> None:
            event.app.exit(result=selected_item().value)

        @key_bindings.add("escape", eager=True)
        @key_bindings.add("c-c", eager=True)
        def cancel(event: object) -> None:
            event.app.exit(result=None)

        root_container = HSplit(
            [
                Window(
                    FormattedTextControl(lambda: [("class:menu-title", menu.title)]),
                    height=1,
                ),
                VSplit(
                    [
                        Frame(radio_list, title="Options"),
                        Frame(
                            Window(
                                FormattedTextControl(
                                    lambda: [("class:menu-preview", preview_text())]
                                ),
                                wrap_lines=False,
                            ),
                            title=menu.preview_title,
                        ),
                    ],
                    padding=1,
                ),
                ConditionalContainer(
                    Window(
                        FormattedTextControl(
                            lambda: [("class:menu-status", menu.status or "")]
                        ),
                        height=1,
                    ),
                    filter=Condition(lambda: bool(menu.status)),
                ),
                Window(
                    FormattedTextControl(lambda: [("class:menu-help", menu.help_text)]),
                    height=1,
                ),
            ]
        )

        return Application(
            layout=Layout(root_container, focused_element=radio_list),
            key_bindings=key_bindings,
            full_screen=True,
            mouse_support=True,
            style=PROMPT_STYLE,
        )

    def select(self, menu: Menu[MenuValue]) -> MenuValue | None:
        application = self._build_application(menu)
        if application is None:
            return None
        return application.run()

    async def select_async(self, menu: Menu[MenuValue]) -> MenuValue | None:
        application = self._build_application(menu)
        if application is None:
            return None
        return await application.run_async()


cli_ui: InteractiveUi = PromptToolkitUi()
