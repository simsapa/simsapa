use tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, INDEXED, STORED};

/// Build the Tantivy schema for sutta indexing with the given language.
pub fn build_sutta_schema(lang: &str) -> Schema {
    let mut builder = Schema::builder();

    let raw_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored();

    let simple_fold_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("simple_fold")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_stem_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_stem"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_normalize_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_normalize"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    builder.add_text_field("uid", raw_opts.clone());
    // Reversed-uid (lowercased uid, character-reversed) for suffix push-down via prefix regex.
    builder.add_text_field("uid_rev", raw_opts.clone());
    builder.add_text_field("title", simple_fold_opts.clone());
    builder.add_text_field("language", raw_opts.clone());
    builder.add_text_field("source_uid", raw_opts.clone());
    builder.add_text_field("sutta_ref", simple_fold_opts);
    builder.add_text_field("nikaya", raw_opts);
    builder.add_text_field("content", lang_stem_opts);
    builder.add_text_field("content_exact", lang_normalize_opts);
    builder.add_bool_field("is_mula", INDEXED | STORED);
    builder.add_bool_field("is_commentary", INDEXED | STORED);
    // A Pāli record whose text is also in the other Pāli edition (a uid in
    // `pali_edition_twins`). The edition preference hides one of the two.
    builder.add_bool_field("has_other_edition", INDEXED | STORED);

    builder.build()
}

/// Build the Tantivy schema for library book chapter indexing with the given language.
pub fn build_library_schema(lang: &str) -> Schema {
    let mut builder = Schema::builder();

    let raw_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored();

    let simple_fold_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("simple_fold")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_stem_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_stem"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_normalize_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_normalize"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    builder.add_text_field("spine_item_uid", raw_opts.clone());
    // Reversed spine_item_uid for suffix push-down via prefix regex.
    builder.add_text_field("spine_item_uid_rev", raw_opts.clone());
    builder.add_text_field("book_uid", raw_opts.clone());
    builder.add_text_field("book_title", simple_fold_opts.clone());
    builder.add_text_field("author", simple_fold_opts.clone());
    builder.add_text_field("title", simple_fold_opts);
    builder.add_text_field("language", raw_opts);
    builder.add_text_field("content", lang_stem_opts);
    builder.add_text_field("content_exact", lang_normalize_opts);

    builder.build()
}

/// Build the Tantivy schema for dictionary word indexing with the given language.
pub fn build_dict_schema(lang: &str) -> Schema {
    let mut builder = Schema::builder();

    let raw_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored();

    let simple_fold_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("simple_fold")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_stem_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_stem"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let lang_normalize_opts = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(&format!("{lang}_normalize"))
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    builder.add_text_field("uid", raw_opts.clone());
    // Reversed-uid (lowercased uid, character-reversed) for suffix push-down via prefix regex.
    builder.add_text_field("uid_rev", raw_opts.clone());
    // For bold_definitions, 'bold' is the equivalent of 'word'.
    builder.add_text_field("word", simple_fold_opts.clone());
    builder.add_text_field("synonyms", simple_fold_opts);
    builder.add_text_field("language", raw_opts.clone());
    // For bold_definitions, 'ref_code' is the equivalent of 'source_uid'.
    builder.add_text_field("source_uid", raw_opts.clone());
    // Used for bold_definitions, a group path constructed as: nikaya / book / title / subhead
    builder.add_text_field("nikaya_group_path", raw_opts);
    builder.add_text_field("content", lang_stem_opts);
    builder.add_text_field("content_exact", lang_normalize_opts);
    builder.add_bool_field("is_bold_definition", INDEXED | STORED);

    builder.build()
}

