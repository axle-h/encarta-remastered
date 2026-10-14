import { Link } from "react-router-dom";
import { useDocumentTitle } from "../components/useDocumentTitle";

const KEYS: [string, string][] = [
  ["Tab / Shift+Tab", "Move between links and buttons; the first Tab offers “Skip to content”"],
  ["Enter, Space or ↓ on a menu", "Open the menu; ↑ ↓ Home End move, Esc closes, ← → go to the next menu"],
  ["Ctrl+F (in an article)", "Find in Article"],
  ["Ctrl+P", "Print: whole article text, with media, or the selection"],
  ["Alt+Home, Alt+← / Alt+→", "Home, Back and Forward"],
  ["Alt+T", "Open the Features menu"],
  ["Ctrl+S", "Go to the search box"],
  ["Ctrl+M, Ctrl+D, Ctrl+T", "World Atlas, Dictionary, Timeline"],
  ["Ctrl+Q", "Geography Quiz (some browsers keep this key)"],
  ["Ctrl+O", "Options"],
  ["← → (media viewer)", "Previous and next picture; Esc closes the viewer"],
  ["Arrow keys, PgUp, PgDn, Home (3D tour)", "Walk, turn, look up and down, look ahead"],
];

/** Help › About and Keyboard Shortcuts. Encarta's own help files describe the Windows program. */
export function HelpPage() {
  useDocumentTitle("Help");
  return (
    <div className="help">
      <h1 className="page-title">About This Encarta</h1>
      <p>
        This is Microsoft Encarta Premium 2009, converted from your own copy of the DVD and shown in a web browser. Articles, media, the
        dictionaries, the timeline, the World Atlas, the 3D virtual tours, the games and Encarta Kids all come from the disc.
      </p>
      <p>
        Encarta's online services (web links, College Prep Online, live news and weather) closed in 2009. Web links still show their 2008
        address and a link to the page as the Internet Archive kept it.
      </p>
      <h2 id="keys">Keyboard shortcuts</h2>
      <table className="help-keys">
        <tbody>
          {KEYS.map(([k, v]) => (
            <tr key={k}>
              <th scope="row">{k}</th>
              <td>{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h2>Your data</h2>
      <p>
        <Link to="/favorites">Favorites</Link>, Geography Quiz scores and MindMaze players are kept in this browser only.
      </p>
    </div>
  );
}
