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

    function sequential_prompt_request(paragraph_idx: int, translation_idx: int, prompt: string) {
        // Silence for less output during qml tests.
    }

    function sequential_word_selection_request(request_id: int, prompt: string) {
        // Silence for less output during qml tests.
    }

    function sequential_prompt_request_with_messages(sender_message_idx: int, messages_json: string) {
        console.log("sequential_prompt_request_with_messages():", sender_message_idx, messages_json);
    }

    function cancel_sequential_requests() {
        // Silence for less output during qml tests.
    }

    signal promptResponse(paragraph_idx: int, translation_idx: int, model: string, response: string);

    signal promptResponseForMessages(message_idx: int, response: string);

    signal wordSelectionResponse(request_id: int, model_name: string, response: string);

    signal sequentialProgress(context_json: string, model_name: string, status: string);
}
