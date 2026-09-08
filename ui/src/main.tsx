import { render } from "preact";
import { App } from "./app";
import { LANG } from "./i18n";
import "./style.css";

document.documentElement.lang = LANG;
// 系统语言在运行中变化时重新取 Locale，避免继续保留旧语言常量。
window.addEventListener("languagechange", () => window.location.reload());
render(<App />, document.getElementById("root")!);
