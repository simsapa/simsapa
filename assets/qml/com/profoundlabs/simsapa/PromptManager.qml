import QtQuick

Item {
    function prompt_request(paragraph_idx: int, translation_idx: int, provider_name: string, model_name: string, prompt: string) {
        // Silence for less output during qml tests.
        // console.log("prompt_request():", paragraph_idx, translation_idx, provider_name, model_name, prompt.slice(0, 30));
    }

    function prompt_request_with_messages(sender_message_idx: int, provider_name: string, model_name: string, messages_json: string) {
        console.log("prompt_request_messages():", sender_message_idx, provider_name, model_name, messages_json);
    }

    function word_selection_request(request_id: int, provider_name: string, model_name: string, prompt: string) {
        // Silence for less output during qml tests.
        // console.log("word_selection_request():", request_id, provider_name, model_name, prompt.slice(0, 30));
    }

    signal promptResponse(paragraph_idx: int, translation_idx: int, model: string, response: string);

    signal promptResponseForMessages(message_idx: int, response: string);

    signal wordSelectionResponse(request_id: int, model_name: string, response: string);
}
