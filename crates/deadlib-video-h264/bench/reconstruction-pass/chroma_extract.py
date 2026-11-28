import pathlib, sys

root = pathlib.Path(__file__).resolve().parent
source = pathlib.Path(sys.argv[1])
s = (source / "chroma.rs").read_text(encoding="utf-8")
# Copy the actual reconstruction fragment, after the all-zero early return,
# before pixel application. Do not invent a separate benchmark algorithm.
start = s.index("        }\n", s.index("        if dc == 0 && !has_ac {")) + len("        }\n")
end = s.index("        let dst = if plane == 0 {", start)
body = s[start:end]
assert "inverse_dct_4x4" in body
(root / "chroma_fragment.rs").write_text(
    "use crate::residual::inverse_dct_4x4;\n"
    "#[allow(unused_variables)]\n"
    "pub fn transform(mut block: [i32; 16], dc: i32, has_ac: bool) -> [i32; 16] {\n"
    + body + "    block\n}\n", encoding="utf-8"
)
