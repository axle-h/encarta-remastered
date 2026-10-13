import { Link } from "react-router-dom";
import { useDocumentTitle } from "../components/useDocumentTitle";

export function NotFound() {
  useDocumentTitle("Page not found");
  return (
    <div className="enc-page-error">
      <h1>Page not found</h1>
      <p>
        There is nothing at this address. <Link to="/">Go to the home page</Link> or search for a topic.
      </p>
    </div>
  );
}
