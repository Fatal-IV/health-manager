import { render, screen } from "@testing-library/react";
import App from "./App";
import { t } from "./i18n";

describe("App", () => {
  it("renders app title", () => {
    render(<App />);
    expect(screen.getByText(t("app.title"))).toBeInTheDocument();
  });
});
