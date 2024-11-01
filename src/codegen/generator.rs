use super::builder::{impl_as_builder_for_struct_or_table, impl_as_builder_for_vector, GenBuilder};
use super::union::GenUnion;
use molecule_codegen::ast::{self, DefaultContent, HasName};

use case::CaseExt;
use std::io;

use core::mem::size_of;

// Little Endian
pub type Number = u32;
// Size of Number
pub const NUMBER_SIZE: usize = size_of::<Number>();

pub(super) trait Generator: HasName + DefaultContent {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()>;
    fn common_generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let struct_name = self.name().to_camel();

        let define = format!(
            r#"
type {struct_name} struct {{
    inner []byte
}}
        "#,
            struct_name = struct_name
        );
        writeln!(writer, "{}", define)?;

        let impl_ = format!(
            r#"
func {struct_name}FromSliceUnchecked(slice []byte) {struct_name} {{
    return {struct_name}{{inner: slice}}
}}
func (s *{struct_name}) AsSlice() []byte {{
    return s.inner
}}
            "#,
            struct_name = struct_name
        );
        writeln!(writer, "{}", impl_)?;

        let default_content = self
            .default_content()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(",");

        let default = format!(
            r#"
func {struct_name}Default() {struct_name} {{
    return {struct_name}FromSliceUnchecked([]byte{{ {default_content} }})
}}
            "#,
            struct_name = struct_name,
            default_content = default_content
        );
        writeln!(writer, "{}", default)?;
        Ok(())
    }
}

impl Generator for ast::Option_ {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let struct_name = self.name().to_camel();
        let inner = self.item().typ().name().to_camel();

        let constructor = format!(
            r#"
func {struct_name}FromSlice(slice []byte, compatible bool) (ret {struct_name}, e error) {{
    if len(slice) == uint32(0) {{
        return ret, errors.None()
    }}

    _, err := {inner_type}FromSlice(slice, compatible)
    if err.NotNone() {{
        return ret, err
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
            struct_name = struct_name,
            inner_type = inner
        );
        writeln!(writer, "{}", constructor)?;

        let impl_ = format!(
            r#"
func (s *{struct_name}) IsSome() bool {{
    return len(s.inner) != uint32(0)
}}
func (s *{struct_name}) IsNone() bool {{
    return len(s.inner) == uint32(0)
}}
func (s *{struct_name}) Into{inner_type}() (ret {inner_type}, e error) {{
	if s.IsNone() {{
		return ret, errors.New("No data")
	}}
	return {inner_type}FromSliceUnchecked(s.AsSlice()), errors.None()
}}
func (s *{struct_name}) AsBuilder() {struct_name}Builder {{
    var ret = New{struct_name}Builder()
    if s.IsSome() {{
        ret.Set({inner_type}FromSliceUnchecked(s.AsSlice()))
    }}
    return ret
}}
            "#,
            struct_name = struct_name,
            inner_type = inner
        );
        writeln!(writer, "{}", impl_)?;
        Ok(())
    }
}

impl Generator for ast::Union {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;
        let struct_name = self.name().to_camel();

        let (union_impl, from_slice_switch_iml) = self.gen_union();
        writeln!(writer, "{}", union_impl)?;

        let struct_constructor = format!(
            r#"
func {struct_name}FromSlice(slice []byte, compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if sliceLen < HeaderSizeUint {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint))}}, " ")
        return ret, errors.New(errMsg)
    }}
    itemID := unpackNumber(slice)
    innerSlice := slice[HeaderSizeUint:]

    switch itemID {{
    {from_slice_switch_iml}
    default:
        return ret, errors.New("UnknownItem, {struct_name}")
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
            struct_name = struct_name,
            from_slice_switch_iml = from_slice_switch_iml
        );
        writeln!(writer, "{}", struct_constructor)?;

        let struct_impl = format!(
            r#"
func (s *{struct_name}) ItemID() Number {{
    return unpackNumber(s.inner)
}}
func (s *{struct_name}) AsBuilder() {struct_name}Builder {{
    ret := New{struct_name}Builder()
    ret.Set(s.ToUnion())
    return ret
}}
            "#,
            struct_name = struct_name
        );
        writeln!(writer, "{}", struct_impl)?;
        Ok(())
    }
}

impl Generator for ast::Array {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let struct_name = self.name().to_camel();
        let inner = self.item().typ().name().to_camel();
        let item_count = self.item_count();
        let total_size = self.total_size();

        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let impl_ = format!(
            r#"
func {struct_name}FromSlice(slice []byte, _compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if sliceLen != uint32({total_size}) {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa({total_size})}}, " ")
        return ret, errors.New(errMsg)
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
        "#,
            struct_name = struct_name,
            total_size = total_size
        );
        writeln!(writer, "{}", impl_)?;

        if self.item().typ().is_byte() {
            writeln!(
                writer,
                r#"
func (s *{struct_name}) RawData() []byte {{
    return s.inner
}}
            "#,
                struct_name = struct_name
            )?
        }

        for i in 0..self.item_count() {
            let func_name = format!("Nth{}", i);
            let start = self.item_size() * i;
            let end = self.item_size() * (i + 1);

            writeln!(
                writer,
                r#"
func (s *{struct_name}) {func_name}() {inner_type} {{
    ret := {inner_type}FromSliceUnchecked(s.inner[{start}:{end}])
    return ret
}}
            "#,
                struct_name = struct_name,
                func_name = func_name,
                inner_type = inner,
                start = start,
                end = end
            )?
        }

        let as_builder_internal = (0..item_count)
            .map(|index| format!("t.Nth{index}(s.Nth{index}())", index = index))
            .collect::<Vec<String>>()
            .join("\n");

        let as_builder = format!(
            r#"
func (s *{struct_name}) AsBuilder() {struct_name}Builder {{
	t := New{struct_name}Builder()
	{as_builder_internal}
	return t
}}
        "#,
            struct_name = struct_name,
            as_builder_internal = as_builder_internal
        );

        writeln!(writer, "{}", as_builder)?;
        Ok(())
    }
}

impl Generator for ast::Struct {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let struct_name = self.name().to_camel();
        let total_size = self.total_size();

        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let impl_ = format!(
            r#"
func {struct_name}FromSlice(slice []byte, _compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if sliceLen != uint32({total_size}) {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa({total_size})}}, " ")
        return ret, errors.New(errMsg)
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
        "#,
            struct_name = struct_name,
            total_size = total_size
        );
        writeln!(writer, "{}", impl_)?;

        let (_, each_getter) = self.fields().iter().zip(self.field_sizes().iter()).fold(
            (0, Vec::with_capacity(self.fields().len())),
            |(mut offset, mut getters), (f, s)| {
                let func_name = f.name().to_camel();
                let inner = f.typ().name().to_camel();

                let start = offset;
                offset += s;
                let end = offset;
                let getter = format!(
                    r#"
func (s *{struct_name}) {func_name}() {inner} {{
    ret := {inner}FromSliceUnchecked(s.inner[{start}:{end}])
    return ret
}}
                "#,
                    struct_name = struct_name,
                    inner = inner,
                    start = start,
                    end = end,
                    func_name = func_name
                );

                getters.push(getter);
                (offset, getters)
            },
        );

        writeln!(writer, "{}", each_getter.join("\n"))?;

        let as_builder = impl_as_builder_for_struct_or_table(&struct_name, self.fields());
        writeln!(writer, "{}", as_builder)?;

        Ok(())
    }
}

impl Generator for ast::FixVec {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let struct_name = self.name().to_camel();
        let inner = self.item().typ().name().to_camel();
        let item_size = self.item_size();

        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let constructor = format!(
            r#"
func {struct_name}FromSlice(slice []byte, _compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if sliceLen < HeaderSizeUint {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint))}}, " ")
        return ret, errors.New(errMsg)
    }}
    itemCount := unpackNumber(slice)
    if itemCount == uint32(0) {{
        if sliceLen != HeaderSizeUint {{
            errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa(uint64(HeaderSizeUint))}}, " ")
            return ret, errors.New(errMsg)
        }}
        return {struct_name}{{inner: slice}}, errors.None()
    }}
    totalSize := uint64(HeaderSizeUint) + uint64(uint32({item_size})*itemCount)
    if uint64(sliceLen) != totalSize {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa(uint64(totalSize))}}, " ")
        return ret, errors.New(errMsg)
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
            struct_name = struct_name,
            item_size = item_size
        );
        writeln!(writer, "{}", constructor)?;

        let impl_ = format!(
            r#"
func (s *{struct_name}) ItemCount() uint64 {{
    number := uint64(unpackNumber(s.inner))
    return number
}}
func (s *{struct_name}) TotalSize() uint64 {{
    return uint64(HeaderSizeUint) + {item_size} * s.ItemCount()
}}
func (s *{struct_name}) Len() uint64 {{
    return s.ItemCount()
}}
func (s *{struct_name}) IsEmpty() bool {{
    return s.Len() == 0
}}
// if {inner_type} is empty, index is out of bounds
func (s *{struct_name}) Get(index uint64) {inner_type} {{
    var re {inner_type}
    if index < s.Len() {{
        start := uint64(HeaderSizeUint) + {item_size}*index
        end := start + {item_size}
        return {inner_type}FromSliceUnchecked(s.inner[start:end])
    }}
    return re
}}
        "#,
            struct_name = struct_name,
            inner_type = inner,
            item_size = item_size
        );
        writeln!(writer, "{}", impl_)?;

        if self.item().typ().is_byte() {
            writeln!(
                writer,
                r#"
func (s *{struct_name}) RawData() []byte {{
    return s.inner[HeaderSizeUint:]
}}
            "#,
                struct_name = struct_name
            )?
        }
        let as_builder = impl_as_builder_for_vector(&struct_name);
        writeln!(writer, "{}", as_builder)?;
        Ok(())
    }
}

impl Generator for ast::DynVec {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let struct_name = self.name().to_camel();
        let inner = self.item().typ().name().to_camel();

        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let constructor = format!(
            r#"
func {struct_name}FromSlice(slice []byte, compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)

    if sliceLen < HeaderSizeUint {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint))}}, " ")
        return ret, errors.New(errMsg)
    }}

    totalSize := unpackNumber(slice)
    if sliceLen != totalSize {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa(uint64(totalSize))}}, " ")
        return ret, errors.New(errMsg)
    }}

    if sliceLen == HeaderSizeUint {{
        return {struct_name}{{inner: slice}}, errors.None()
    }}

    if sliceLen < HeaderSizeUint*uint32(2) {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}

    offsetFirst := unpackNumber(slice[HeaderSizeUint:])
	offsetSize := offsetFirst%HeaderSizeUint
    if offsetSize != uint32(0) {{
		errMsg := strings.Join([]string{{"OffsetsNotMatch", "{struct_name}", strconv.Itoa(uint64(offsetFirst%uint32(4))), "!= 0", strconv.Itoa(uint64(offsetFirst)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}
	headerSize := HeaderSizeUint*uint32(2)
	if offsetFirst < headerSize {{
		errMsg := strings.Join([]string{{"OffsetsNotMatch", "{struct_name}", strconv.Itoa(uint64(offsetFirst%uint32(4))), "!= 0", strconv.Itoa(uint64(offsetFirst)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}

    if sliceLen < offsetFirst {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(offsetFirst))}}, " ")
        return ret, errors.New(errMsg)
    }}
    itemCount := uint32(offsetFirst)/HeaderSizeUint - uint32(1)

    offsets := make([]uint32, itemCount)

    for i := uint32(0); i < itemCount; i++ {{
        offsets[i] = uint32(unpackNumber(slice[HeaderSizeUint:][HeaderSizeUint*i:]))
    }}

    offsets = append(offsets, uint32(totalSize))

    for i := 0; i < uint64(len(offsets)); i++ {{
        bit := i & 1
		c1 := bit != 0
		c2 := offsets[i-1] > offsets[i]
		if c1 && c2 {{
            errMsg := strings.Join([]string{{"OffsetsNotMatch", "{struct_name}"}}, " ")
            return ret, errors.New(errMsg)
        }}
    }}

    for i := 0; i < uint64(len(offsets)); i++ {{
        bit := i & 1
		if bit != 0 {{
            start := offsets[i-1]
            end := offsets[i]
            _, err := {inner_type}FromSlice(slice[start:end], compatible)

            if err.NotNone() {{
                return ret, err
            }}
        }}
    }}

    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
            struct_name = struct_name,
            inner_type = inner
        );
        writeln!(writer, "{}", constructor)?;

        let impl_ = format!(
            r#"
func (s *{struct_name}) TotalSize() uint64 {{
    return uint64(unpackNumber(s.inner))
}}
func (s *{struct_name}) ItemCount() uint64 {{
    var number uint64 = 0
    if uint32(s.TotalSize()) == HeaderSizeUint {{
        return number
    }}
    number = uint64(unpackNumber(s.inner[HeaderSizeUint:]))/4 - 1
    return number
}}
func (s *{struct_name}) Len() uint64 {{
    return s.ItemCount()
}}
func (s *{struct_name}) IsEmpty() bool {{
    return s.Len() == 0
}}
// if {inner_type} is empty, index is out of bounds
func (s *{struct_name}) Get(index uint64) {inner_type} {{
    if index < s.Len() {{
        start_index := uint64(HeaderSizeUint) * (1 + index)
        start := unpackNumber(s.inner[start_index:])

        if index == s.Len()-1 {{
            return {inner_type}FromSliceUnchecked(s.inner[start:])
        }} else {{
            end_index := start_index + uint64(HeaderSizeUint)
            end := unpackNumber(s.inner[end_index:])
            return {inner_type}FromSliceUnchecked(s.inner[start:end])
        }}
    }}
    var b {inner_type}
    return b
}}
            "#,
            struct_name = struct_name,
            inner_type = inner
        );
        writeln!(writer, "{}", impl_)?;
        let as_builder = impl_as_builder_for_vector(&struct_name);
        writeln!(writer, "{}", as_builder)?;
        Ok(())
    }
}

impl Generator for ast::Table {
    fn generate<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let field_count = self.fields().len();
        let struct_name = self.name().to_camel();

        self.common_generate(writer)?;
        writeln!(writer, "{}", self.gen_builder())?;

        let constructor = if self.fields().is_empty() {
            format!(
                r#"
func New{struct_name}() {struct_name} {{
    var s bytes.Buffer
    s.Write(packNumber(Number(HeaderSizeUint)))
    return {struct_name}{{inner: s.Bytes()}}
}}
func {struct_name}FromSlice(slice []byte, compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if uint32(sliceLen) < HeaderSizeUint {{
        return ret, errors.New("HeaderIsBroken")
    }}

    totalSize := unpackNumber(slice)
    if Number(sliceLen) != totalSize {{
        return ret, errors.New("TotalSizeNotMatch")
    }}

    if uint32(sliceLen) > HeaderSizeUint && !compatible {{
        return ret, errors.New("FieldCountNotMatch")
    }}
    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
                struct_name = struct_name
            )
        } else {
            let verify_fields = self
                .fields()
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let field = f.typ().name().to_camel();
                    let start = i;
                    let end = i + 1;
                    format!(
                        r#"
_, err = {field}FromSlice(slice[offsets[{start}]:offsets[{end}]], compatible)
if err.NotNone() {{
    return ret, err
}}
                "#,
                        field = field,
                        start = start,
                        end = end
                    )
                })
                .collect::<Vec<String>>()
                .join("\n");

            format!(
                r#"
func {struct_name}FromSlice(slice []byte, compatible bool) (ret {struct_name}, e error) {{
    sliceLen := len(slice)
    if uint32(sliceLen) < HeaderSizeUint {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint))}}, " ")
        return ret, errors.New(errMsg)
    }}

    totalSize := unpackNumber(slice)
    if Number(sliceLen) != totalSize {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "!=", strconv.Itoa(uint64(totalSize))}}, " ")
        return ret, errors.New(errMsg)
    }}

    if uint32(sliceLen) < HeaderSizeUint*uint32(2) {{
        errMsg := strings.Join([]string{{"TotalSizeNotMatch", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}

    offsetFirst := unpackNumber(slice[HeaderSizeUint:])
	offsetSize := offsetFirst%HeaderSizeUint
    if offsetSize != uint32(0) {{
		errMsg := strings.Join([]string{{"OffsetsNotMatch", "{struct_name}", strconv.Itoa(uint64(offsetFirst%uint32(4))), "!= 0", strconv.Itoa(uint64(offsetFirst)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}
	headerSize := HeaderSizeUint*uint32(2)
	if offsetFirst < headerSize {{
		errMsg := strings.Join([]string{{"OffsetsNotMatch", "{struct_name}", strconv.Itoa(uint64(offsetFirst%uint32(4))), "!= 0", strconv.Itoa(uint64(offsetFirst)), "<", strconv.Itoa(uint64(HeaderSizeUint*uint32(2)))}}, " ")
        return ret, errors.New(errMsg)
    }}

    if sliceLen < offsetFirst {{
        errMsg := strings.Join([]string{{"HeaderIsBroken", "{struct_name}", strconv.Itoa(uint64(sliceLen)), "<", strconv.Itoa(uint64(offsetFirst))}}, " ")
        return ret, errors.New(errMsg)
    }}

    fieldCount := uint32(offsetFirst)/HeaderSizeUint - uint32(1)
    if fieldCount < uint32({field_count}) {{
        return ret, errors.New("FieldCountNotMatch")
    }} else if !compatible && fieldCount > uint32({field_count}) {{
        return ret, errors.New("FieldCountNotMatch")
    }}

    offsets := make([]uint32, fieldCount)

    for i := uint32(0); i < fieldCount; i++ {{
        offsets[i] = uint32(unpackNumber(slice[HeaderSizeUint:][HeaderSizeUint*i:]))
    }}
    offsets = append(offsets, totalSize)

    for i := 0; i < uint64(len(offsets)); i++ {{
        bit := i & 1
		c1 := bit != 0
		c2 := offsets[i-1] > offsets[i]
		if c1 && c2 {{
            return ret, errors.New("OffsetsNotMatch")
        }}
    }}

    var err error
    {verify_fields}

    return {struct_name}{{inner: slice}}, errors.None()
}}
            "#,
                struct_name = struct_name,
                field_count = field_count,
                verify_fields = verify_fields
            )
        };
        writeln!(writer, "{}", constructor)?;

        let impl_ = format!(
            r#"
func (s *{struct_name}) TotalSize() uint64 {{
    return uint64(unpackNumber(s.inner))
}}
func (s *{struct_name}) FieldCount() uint64 {{
    var number uint64 = 0
    if uint32(s.TotalSize()) == HeaderSizeUint {{
        return number
    }}
    number = uint64(unpackNumber(s.inner[HeaderSizeUint:]))/4 - 1
    return number
}}
func (s *{struct_name}) Len() uint64 {{
    return s.FieldCount()
}}
func (s *{struct_name}) IsEmpty() bool {{
    return s.Len() == 0
}}
func (s *{struct_name}) CountExtraFields() uint64 {{
    return s.FieldCount() - {field_count}
}}

func (s *{struct_name}) HasExtraFields() bool {{
    return {field_count} != s.FieldCount()
}}
            "#,
            struct_name = struct_name,
            field_count = field_count,
        );
        writeln!(writer, "{}", impl_)?;

        let (getter_stmt_last, getter_stmt) = {
            let getter_stmt_last = "s.inner[start:]".to_string();
            let getter_stmt = "s.inner[start:end]".to_string();
            (getter_stmt_last, getter_stmt)
        };
        let each_getter = self
            .fields()
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let func = f.name().to_camel();

                let inner = f.typ().name().to_camel();
                let start = (i + 1) * NUMBER_SIZE;
                let end = (i + 2) * NUMBER_SIZE;
                if i == self.fields().len() - 1 {
                    format!(
                        r#"
func (s *{struct_name}) {func}() {inner} {{
    var ret {inner}
    start := unpackNumber(s.inner[{start}:])
    if s.HasExtraFields() {{
        end := unpackNumber(s.inner[{end}:])
        ret = {inner}FromSliceUnchecked({getter_stmt})
    }} else {{
        ret = {inner}FromSliceUnchecked({getter_stmt_last})
    }}
    return ret
}}
                        "#,
                        struct_name = struct_name,
                        start = start,
                        end = end,
                        func = func,
                        inner = inner,
                        getter_stmt = getter_stmt,
                        getter_stmt_last = getter_stmt_last
                    )
                } else {
                    format!(
                        r#"
func (s *{struct_name}) {func}() {inner} {{
    start := unpackNumber(s.inner[{start}:])
    end := unpackNumber(s.inner[{end}:])
    return {inner}FromSliceUnchecked({getter_stmt})
}}
               "#,
                        struct_name = struct_name,
                        func = func,
                        inner = inner,
                        getter_stmt = getter_stmt,
                        start = start,
                        end = end
                    )
                }
            })
            .collect::<Vec<_>>();
        writeln!(writer, "{}", each_getter.join("\n"))?;

        let as_builder = impl_as_builder_for_struct_or_table(&struct_name, self.fields());
        writeln!(writer, "{}", as_builder)?;
        Ok(())
    }
}
