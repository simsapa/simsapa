// @generated automatically by Diesel CLI.

diesel::table! {
    app_settings (id) {
        id -> Integer,
        key -> Text,
        value -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    suttas (id) {
        id -> Integer,
        uid -> Text,
        sutta_ref -> Text,
        nikaya -> Text,
        language -> Text,
        group_path -> Nullable<Text>,
        group_index -> Nullable<Integer>,
        order_index -> Nullable<Integer>,
        sutta_range_group -> Nullable<Text>,
        sutta_range_start -> Nullable<Integer>,
        sutta_range_end -> Nullable<Integer>,
        title -> Nullable<Text>,
        title_ascii -> Nullable<Text>,
        title_pali -> Nullable<Text>,
        title_trans -> Nullable<Text>,
        description -> Nullable<Text>,
        content_plain -> Nullable<Text>,
        content_html -> Nullable<Text>,
        content_json -> Nullable<Text>,
        content_json_tmpl -> Nullable<Text>,
        source_uid -> Nullable<Text>,
        source_info -> Nullable<Text>,
        source_language -> Nullable<Text>,
        message -> Nullable<Text>,
        copyright -> Nullable<Text>,
        license -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
        // indexed_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    sutta_variants (id) {
        id -> Integer,
        sutta_id -> Integer,
        sutta_uid -> Text,
        language -> Nullable<Text>,
        source_uid -> Nullable<Text>,
        content_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    sutta_comments (id) {
        id -> Integer,
        sutta_id -> Integer,
        sutta_uid -> Text,
        language -> Nullable<Text>,
        source_uid -> Nullable<Text>,
        content_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    sutta_glosses (id) {
        id -> Integer,
        sutta_id -> Integer,
        sutta_uid -> Text,
        language -> Nullable<Text>,
        source_uid -> Nullable<Text>,
        content_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    books (id) {
        id -> Integer,
        uid -> Text,
        document_type -> Text,
        title -> Nullable<Text>,
        author -> Nullable<Text>,
        language -> Nullable<Text>,
        file_path -> Nullable<Text>,
        metadata_json -> Nullable<Text>,
        enable_embedded_css -> Bool,
        toc_json -> Nullable<Text>,
        is_user_added -> Bool,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    book_spine_items (id) {
        id -> Integer,
        book_id -> Integer,
        book_uid -> Text,
        spine_item_uid -> Text,
        spine_index -> Integer,
        resource_path -> Text,
        title -> Nullable<Text>,
        language -> Nullable<Text>,
        content_html -> Nullable<Text>,
        content_plain -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    book_resources (id) {
        id -> Integer,
        book_id -> Integer,
        book_uid -> Text,
        resource_path -> Text,
        mime_type -> Nullable<Text>,
        content_data -> Nullable<Binary>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    chanting_collections (id) {
        id -> Integer,
        uid -> Text,
        title -> Text,
        description -> Nullable<Text>,
        language -> Text,
        sort_index -> Integer,
        is_user_added -> Bool,
        metadata_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    chanting_chants (id) {
        id -> Integer,
        uid -> Text,
        collection_uid -> Text,
        title -> Text,
        description -> Nullable<Text>,
        sort_index -> Integer,
        is_user_added -> Bool,
        metadata_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    chanting_sections (id) {
        id -> Integer,
        uid -> Text,
        chant_uid -> Text,
        title -> Text,
        content_pali -> Text,
        sort_index -> Integer,
        is_user_added -> Bool,
        metadata_json -> Nullable<Text>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    chanting_recordings (id) {
        id -> Integer,
        uid -> Text,
        section_uid -> Text,
        file_name -> Text,
        recording_type -> Text,
        label -> Nullable<Text>,
        duration_ms -> Integer,
        markers_json -> Nullable<Text>,
        volume -> Float,
        playback_position_ms -> Integer,
        waveform_json -> Nullable<Text>,
        is_user_added -> Bool,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    bookmark_folders (id) {
        id -> Integer,
        name -> Text,
        sort_order -> Integer,
        is_last_session -> Bool,
        is_user_added -> Bool,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    bookmark_items (id) {
        id -> Integer,
        folder_id -> Integer,
        item_uid -> Text,
        table_name -> Text,
        title -> Nullable<Text>,
        tab_group -> Text,
        scroll_position -> Float,
        find_query -> Text,
        find_match_index -> Integer,
        sort_order -> Integer,
        is_user_added -> Bool,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    gloss_prompts_history (id) {
        id -> Integer,
        item_type -> Text,
        data_json -> Text,
        created_at -> Nullable<Timestamp>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    gloss_word_context_cache (id) {
        id -> Integer,
        word -> Text,
        context_hash -> Text,
        context_snippet -> Text,
        selected_uid -> Text,
        origin -> Text,
        built_in -> Integer,
        deconstruction -> Nullable<Text>,
        created_at -> Nullable<Timestamp>,
        updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    gloss_phrase_selections (id) {
        id -> Integer,
        phrase -> Text,
        word -> Text,
        selected_uid -> Text,
    }
}

diesel::joinable!(sutta_variants -> suttas (sutta_id));
diesel::joinable!(sutta_comments -> suttas (sutta_id));
diesel::joinable!(sutta_glosses -> suttas (sutta_id));
diesel::joinable!(book_spine_items -> books (book_id));
diesel::joinable!(book_resources -> books (book_id));
diesel::joinable!(bookmark_items -> bookmark_folders (folder_id));

diesel::allow_tables_to_appear_in_same_query!(
    app_settings,
    suttas,
    sutta_variants,
    sutta_comments,
    sutta_glosses,
    books,
    book_spine_items,
    book_resources,
    chanting_collections,
    chanting_chants,
    chanting_sections,
    chanting_recordings,
    bookmark_folders,
    bookmark_items,
    gloss_prompts_history,
    gloss_word_context_cache,
    gloss_phrase_selections,
);
