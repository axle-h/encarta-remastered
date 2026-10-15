// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { plainSpaces, rangeToMarkdown, toMarkdown } from "./markdown";

const mount = (html: string) => {
  const el = document.createElement("div");
  el.innerHTML = html;
  document.body.appendChild(el);
  return el;
};

afterEach(() => {
  document.body.innerHTML = "";
});

describe("article Markdown", () => {
  it("keeps headings, emphasis, lists, tables and web links; drops controls and media", () => {
    const el = mount(`
      <div class="enc-article">
        <section><div class="enc-section-head"><button>-</button><h2 class="enc-section-title">Life Cycle</h2></div>
          <p class="enc-p">Females lay 20 to 90 <b>eggs</b> in <i>nests</i>, 8&nbsp;feet away.</p>
          <aside class="enc-media"><span>Sidebars</span></aside>
          <ul><li>One</li><li>Two <a href="https://example.org/a">site</a></li></ul>
          <div class="enc-inline-table"><table><tr><th>Name</th><th>Size</th></tr><tr><td>Nile</td><td>6 m</td></tr></table></div>
          <p class="enc-p">A <a href="/kids/item/5">link</a> with *stars*<br>and a break.</p>
        </section>
      </div>`);
    expect(toMarkdown(el)).toBe(
      [
        "## Life Cycle",
        "Females lay 20 to 90 **eggs** in *nests*, 8 feet away.",
        "- One\n- Two [site](https://example.org/a)",
        "| Name | Size |\n| --- | --- |\n| Nile | 6 m |",
        "A link with \\*stars\\*  \nand a break.",
      ].join("\n\n"),
    );
  });

  it("copies only the selected part", () => {
    const el = mount(`<h2>Title</h2><p>First para.</p><p>Second <b>bold</b> para.</p>`);
    const range = document.createRange();
    range.setStart(el.querySelectorAll("p")[0]!.firstChild!, 6);
    range.setEnd(el.querySelector("b")!.firstChild!, 2);
    expect(rangeToMarkdown(range)).toBe("para.\n\nSecond **bo**");
  });

  it("turns no-break spaces into plain spaces", () => {
    expect(plainSpaces("8 feet wide")).toBe("8 feet wide");
  });
});
