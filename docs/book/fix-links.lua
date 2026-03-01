-- Pandoc Lua filter for PDF book generation
-- 1. Converts "Part X:" headings into LaTeX \part{} commands
-- 2. Promotes "Chapter N:" headings from level 2 to level 1
-- 3. Drops the top-level book title heading (handled by front matter)
-- 4. Strips "Chapter N:" prefixes so LaTeX auto-numbers chapters
-- 5. Drops "APPENDICES" heading (rendered as a \part)
-- 6. Removes HTML blocks/inlines that don't work in LaTeX

function Header(el)
  local text = pandoc.utils.stringify(el)

  -- Drop the book title heading (already in front matter)
  if el.level == 1 and text == "Programming Forge" then
    return {}
  end

  -- Convert Part headings to LaTeX \part{}
  if el.level == 1 and text:match("^[Pp][Aa][Rr][Tt]%s+[IVX]+") then
    -- Extract just the part name after "Part X:"
    local part_name = text:match("^[Pp][Aa][Rr][Tt]%s+[IVX]+:%s*(.*)")
    if not part_name then
      part_name = text
    end
    return pandoc.RawBlock("latex", "\\part{" .. part_name .. "}")
  end

  -- Convert APPENDICES heading to a \part{}
  if el.level == 1 and text:match("^APPENDICES") then
    return pandoc.RawBlock("latex", "\\part{Appendices}")
  end

  -- Promote Chapter headings: level 2 -> level 1, strip "Chapter N:" prefix
  if el.level == 2 and text:match("^Chapter%s+%d+") then
    -- Strip "Chapter N:" prefix - LaTeX will auto-number
    local chapter_title = text:match("^Chapter%s+%d+:%s*(.*)")
    if chapter_title then
      return pandoc.Header(1, pandoc.Str(chapter_title), el.attr)
    else
      el.level = 1
      return el
    end
  end

  -- Promote all other sub-headings by one level (### -> ##, #### -> ###)
  -- since chapters went from 2 -> 1
  if el.level >= 3 then
    el.level = el.level - 1
    return el
  end

  return el
end

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
