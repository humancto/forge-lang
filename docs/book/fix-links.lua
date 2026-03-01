-- Pandoc Lua filter to fix internal links and clean up for PDF
-- Removes HTML alignment tags that don't work in LaTeX

function RawBlock(el)
  -- Drop HTML blocks (alignment divs, badges, etc.)
  if el.format == "html" then
    return {}
  end
end

function RawInline(el)
  if el.format == "html" then
    return {}
  end
end
