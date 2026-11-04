// @generated automatically by Diesel CLI.

// In the order of the database's columns, the large text last. `DictWord`
// maps its fields by position, so it follows this order too.
diesel::table! {
    dict_words (id) {
        id -> Integer,
        dictionary_id -> Integer,
        dict_label -> Text,
        uid -> Text,
        word -> Text,
        word_ascii -> Text,
        language -> Nullable<Text>,
        meaning_order -> Nullable<Integer>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
        // indexed_at -> Nullable<Timestamp>,
        definition_plain -> Nullable<Text>,
        word_nom_sg -> Nullable<Text>,
        phonetic -> Nullable<Text>,
        transliteration -> Nullable<Text>,
        synonyms -> Nullable<Text>,
        antonyms -> Nullable<Text>,
        homonyms -> Nullable<Text>,
        also_written_as -> Nullable<Text>,
        see_also -> Nullable<Text>,
        summary -> Nullable<Text>,
        inflections -> Nullable<Text>,
        definition_html -> Nullable<Text>,
    }
}

diesel::table! {
    dictionaries (id) {
        id -> Integer,
        label -> Text,
        title -> Text,
        dict_type -> Text,
        creator -> Nullable<Text>,
        description -> Nullable<Text>,
        feedback_email -> Nullable<Text>,
        feedback_url -> Nullable<Text>,
        version -> Nullable<Text>,
        is_user_imported -> Bool,
        language -> Nullable<Text>,
        indexed_at -> Nullable<Timestamp>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    dict_resources (id) {
        id -> Integer,
        dictionary_id -> Integer,
        resource_path -> Text,
        mime_type -> Nullable<Text>,
        content_data -> Nullable<Binary>,
        // created_at -> Nullable<Timestamp>,
        // updated_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(dict_words -> dictionaries (dictionary_id));
diesel::joinable!(dict_resources -> dictionaries (dictionary_id));

diesel::allow_tables_to_appear_in_same_query!(
    dict_words,
    dictionaries,
    dict_resources,
);
