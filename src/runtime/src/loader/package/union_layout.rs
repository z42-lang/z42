use crate::object::value::union_blob::is_storage_name;
use crate::zpkg::ir::ClassDesc;
use anyhow::{ensure, Result};
use rustc_hash::FxHashMap;

pub(super) fn validate_fields(classes: &[ClassDesc]) -> Result<()> {
    let mut unions = None;
    for class in classes {
        if !class.fields.iter().any(|field| is_storage_name(&field.type_tag)) {
            continue;
        }
        let unions = unions.get_or_insert_with(|| {
            classes.iter().filter_map(|entry| {
                Some((entry.name.as_str(), entry.union_layout.as_deref()?))
            }).collect::<FxHashMap<_, _>>()
        });
        let object = class.object_layout.as_ref().ok_or_else(|| {
            anyhow::anyhow!("inline union fields of `{}` require an object layout", class.name)
        })?;
        ensure!(class.fields.len() == object.field_offsets.len()
            && class.fields.len() == object.field_sizes.len()
            && class.fields.len() == object.field_kinds.len(),
            "incomplete inline union field layout for `{}`", class.name);

        let mut end = 0;
        for (i, field) in class.fields.iter().enumerate() {
            let offset = object.field_offsets[i];
            let size = object.field_sizes[i];
            ensure!(offset >= end && offset.checked_add(size).is_some_and(|end| end <= object.size),
                "overlapping or out-of-bounds field `{}.{}`", class.name, field.name);
            end = offset + size;
            if !is_storage_name(&field.type_tag) { continue; }
            let layout = unions.get(field.type_tag.as_str()).ok_or_else(|| {
                anyhow::anyhow!("missing inline union layout `{}`", field.type_tag)
            })?;
            let root = layout.root();
            ensure!(object.field_kinds[i] == 3 && size == root.size && offset % root.align as u32 == 0,
                "inline union field `{}.{}` does not match its type layout", class.name, field.name);
            ensure!(object.ref_offsets.iter().all(|&reference| reference >= end
                || reference.checked_add(8).is_some_and(|end| end <= offset)),
                "static reference overlaps inline union field `{}.{}`", class.name, field.name);
        }
    }
    Ok(())
}
