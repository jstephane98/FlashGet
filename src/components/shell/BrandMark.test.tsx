import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { BrandMark } from "./BrandMark";

describe("BrandMark", () => {
  it("utilise la version simplifiée en petit et la complète en grand", () => {
    const small = render(<BrandMark size={20} />).container;
    expect(small.querySelectorAll("g rect")).toHaveLength(2);
    const big = render(<BrandMark size={64} />).container;
    expect(big.querySelectorAll("g rect")).toHaveLength(6);
  });

  it("donne à chaque logo son propre dégradé", () => {
    const { container } = render(<><BrandMark size={20} /><BrandMark size={20} /></>);
    const ids = [...container.querySelectorAll("linearGradient")].map((g) => g.id);
    expect(new Set(ids).size).toBe(2);
    const tile = container.querySelector("svg > rect");
    expect(tile?.getAttribute("fill")).toBe(`url(#${ids[0]})`);
  });
});
