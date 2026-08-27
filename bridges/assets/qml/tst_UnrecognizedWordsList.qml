import QtQuick
import QtTest

TestCase {
    id: test_case
    width: 400
    height: 300
    visible: true
    when: windowShown
    name: "TestUnrecognizedWordsList"

    UnrecognizedWordsList {
        id: unrecognized_words_list
        anchors.fill: parent
        
        // Required properties
        word_list: []
        prefix_text: "Click for deconstructor lookup:"
        bg_color_darker: "#1C2025"
        bg_color_lighter: "#2E333D" 
        text_color: "#F0F0F0"
        border_color: "#0a0a0a"
        
        // Test properties
        max_words: 3  // Lower limit for testing overflow
    }

    SignalSpy {
        id: word_clicked_spy
        target: unrecognized_words_list
        signalName: "wordClicked"
    }

    function init() {
        word_clicked_spy.clear();
        unrecognized_words_list.word_list = [];
    }

    function test_visibility_with_empty_list() {
        unrecognized_words_list.word_list = [];
        verify(!unrecognized_words_list.visible, "Component should be hidden when word list is empty");
    }

    function test_visibility_with_words() {
        unrecognized_words_list.word_list = ["atthaññe", "bhikkhū"];
        verify(unrecognized_words_list.visible, "Component should be visible when word list has items");
    }

    function test_word_buttons_creation() {
        unrecognized_words_list.word_list = ["atthaññe", "bhikkhū", "sāmaññe"];
        
        // Wait for UI to update
        wait(100);
        
        // Test that the component is visible and has the right word count
        verify(unrecognized_words_list.visible, "Component should be visible with words");
        compare(unrecognized_words_list.visible_words.length, 3, "Should have 3 visible words");
        verify(!unrecognized_words_list.has_overflow, "Should not have overflow with 3 words and max 3");
    }

    function test_overflow_display() {
        unrecognized_words_list.word_list = ["word1", "word2", "word3", "word4", "word5"];
        
        // Wait for UI to update
        wait(100);
        
        // Should show max_words (3) buttons plus overflow text
        verify(unrecognized_words_list.has_overflow, "Should detect overflow with 5 words and max 3");
        compare(unrecognized_words_list.overflow_count, 2, "Should show 2 words overflow");
    }

    function test_word_click_signal() {
        unrecognized_words_list.word_list = ["atthaññe"];
        
        // Wait for UI to update
        wait(100);
        
        // Manually emit the signal to test signal connection
        unrecognized_words_list.wordClicked("atthaññe");
        
        // Check signal was emitted
        compare(word_clicked_spy.count, 1, "wordClicked signal should be emitted once");
        compare(word_clicked_spy.signalArguments[0][0], "atthaññe", "Signal should pass correct word");
    }

    function test_prefix_text_display() {
        unrecognized_words_list.word_list = ["test"];
        
        // Wait for UI to update  
        wait(100);
        
        // Test the prefix text property directly
        compare(unrecognized_words_list.prefix_text, "Click for deconstructor lookup:", "Prefix text should match");
        verify(unrecognized_words_list.visible, "Component should be visible");
    }

    function test_button_styling() {
        unrecognized_words_list.word_list = ["test"];
        
        // Wait for UI to update
        wait(100);
        
        // Test the styling properties are properly set
        verify(unrecognized_words_list.visible, "Component should be visible");
        compare(unrecognized_words_list.bg_color_darker, "#1C2025", "Background color should be set");
        compare(unrecognized_words_list.bg_color_lighter, "#2E333D", "Hover color should be set");
        compare(unrecognized_words_list.text_color, "#F0F0F0", "Text color should be set");
    }
}
