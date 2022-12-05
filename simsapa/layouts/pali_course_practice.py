import re
import uuid
from pathlib import Path
import random
import json
from typing import Dict, List, Optional
from functools import partial
import markdown

from sqlalchemy.sql import func

from PyQt6 import QtWidgets
from PyQt6.QtMultimedia import QAudioDevice, QSoundEffect
from PyQt6.QtCore import QSize, QTimer, QUrl, pyqtSignal, Qt
from PyQt6.QtGui import QEnterEvent, QIcon, QPixmap
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings

from PyQt6.QtWidgets import QFrame, QHBoxLayout, QLabel, QSizePolicy, QMessageBox, QPushButton, QSpacerItem, QVBoxLayout

from simsapa import BUTTON_BG_COLOR, COURSES_DIR, IS_MAC, READING_BACKGROUND_COLOR, SIMSAPA_PACKAGE_DIR, DbSchemaName, logger
from simsapa.layouts.html_content import html_page
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.pali_course_helpers import get_remaining_challenges_in_group

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, AppWindowInterface, PaliChallengeType, PaliCourseGroup, PaliItem, QExpanding, QMinimum, UChallenge, UChallengeGroup


class ChoiceButton(QPushButton):

    choice_clicked = pyqtSignal(dict)
    play_audio = pyqtSignal(Path)

    pali_item: PaliItem
    audio_path: Optional[Path] = None

    _play_timer = QTimer()

    def __init__(self, pali_item: PaliItem, course_dir: Optional[Path] = None, enable_audio = True, parent=None):
        self.pali_item = pali_item

        super().__init__(text=self.pali_item['text'], parent=parent)

        self.setMinimumSize(8*len(self.pali_item['text']), 40)

        # Challenge asset paths are relative to course dir
        if course_dir and self.pali_item['audio'] and enable_audio:
            p = course_dir.joinpath(self.pali_item['audio'])
            if p.exists():
                self.audio_path = p

                icon = QIcon()
                icon.addPixmap(QPixmap(":/volume-high"))
                self.setIcon(icon)
                self.setIconSize(QSize(12, 12))
            else:
                logger.warn(f"Audio missing: {p}")

        self.clicked.connect(partial(self._handle_clicked))


    def _handle_clicked(self):
        self.choice_clicked.emit(self.pali_item)


    def _emit_play(self):
        self.play_audio.emit(self.audio_path)


    def do_remove(self):
        if self._play_timer.isActive():
            self._play_timer.stop()
        self.deleteLater()


    def enterEvent(self, e: QEnterEvent):
        if self.isEnabled():
            self.setStyleSheet(f"background-color: {BUTTON_BG_COLOR}; color: white;")

        if self.audio_path and self.isEnabled() and not self._play_timer.isActive():
            self._play_timer = QTimer()
            self._play_timer.timeout.connect(partial(self._emit_play))
            self._play_timer.setSingleShot(True)
            self._play_timer.start(500)

        return super().enterEvent(e)


    def leaveEvent(self, e):
        if self._play_timer.isActive():
            self._play_timer.stop()

        self.setStyleSheet("")
        return super().leaveEvent(e)


class CoursePracticeWindow(AppWindowInterface):

    finished = pyqtSignal()

    current_group: UChallengeGroup
    progress_in_remaining: int = 1
    challenges: List[UChallenge] = []

    current_challenge: UChallenge
    current_challenge_idx = 0

    choice_buttons: List[ChoiceButton] = []
    answer_buttons: List[ChoiceButton] = []

    def __init__(self, app_data: AppData, group: PaliCourseGroup, parent=None) -> None:
        super().__init__(parent)
        logger.info("CoursePracticeWindow()")

        self._app_data: AppData = app_data

        self.setStyleSheet(f"background-color: {READING_BACKGROUND_COLOR};")

        g = self._get_group(group)
        if g:
            self.current_group = g
            self.current_challenge_idx = 0
            self.challenges = get_remaining_challenges_in_group(self._app_data, self.current_group)
            self._set_current_challenge()

        else:
            logger.error(f"Can't load group: {group}")
            return

        self._setup_audio()
        self._ui_setup()
        self._load_challenge_content()
        self._connect_signals()


    def _setup_audio(self):
        self.player = QSoundEffect()
        self.audio_device = QAudioDevice()
        self.player.setAudioDevice(self.audio_device)

        volume = self._app_data.app_settings.get('audio_volume', 1.0)
        if volume == 0.0:
            self.player.setMuted(True)
        else:
            self.player.setMuted(False)
            self.player.setVolume(volume)


    def _play_audio(self, audio_path: Path):
        if audio_path.exists():
            if self.player.isMuted():
                return

            if self.player.isPlaying():
                return

            # FIXME When a source hasn't finished playing, and a new play is
            # requested, it makes a loud noise, even if .stop() is called before
            # playing.
            #
            # A way to test this problem is when a ChoiceButton started playing
            # a word, then we check to finish while it's playing, and the
            # _play_answer() will try to start playing.

            self.player.setSource(QUrl.fromLocalFile(str(audio_path)))
            self.player.play()

        else:
            logger.warn(f"Audio missing: {audio_path}")


    def _set_current_challenge(self):
        if self.current_challenge_idx < len(self.challenges):
            self.current_challenge = self.challenges[self.current_challenge_idx]


    def _set_next_challenge(self) -> bool:
        if self.current_challenge_idx < len(self.challenges) - 1: # type: ignore
            self.current_challenge_idx += 1
            self._set_current_challenge()
            return True

        else:
            return False

    def _get_group(self, group: PaliCourseGroup) -> Optional[UChallengeGroup]:
        r: Optional[UChallengeGroup] = None

        if group['db_schema'] == DbSchemaName.AppData.value:
            r = self._app_data.db_session \
                .query(Am.ChallengeGroup) \
                .filter(Am.ChallengeGroup.id == group['db_id']) \
                .first()


        else:
            r = self._app_data.db_session \
                .query(Um.ChallengeGroup) \
                .filter(Um.ChallengeGroup.id == group['db_id']) \
                .first()

        return r


    def _ui_setup(self):
        self.setWindowTitle("Pali Courses Browser")
        self.resize(850, 650)


        if IS_MAC:
            font_family = "Helvetica"
        else:
            font_family = "DejaVu Sans"

        title_style = f"font-family: {font_family}; font-weight: bold; font-size: 14pt;"
        text_style = f"font-family: {font_family}; font-weight: normal; font-size: 12pt;"

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        # === Title ===

        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QMinimum))

        # use Vbox and show group (x/y)

        self.title_wrap = QHBoxLayout()
        self._layout.addLayout(self.title_wrap)

        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QExpanding))

        self.title_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.title_box = QVBoxLayout()
        self.title_wrap.addLayout(self.title_box)

        self.group_title = QLabel()
        self.group_title.setStyleSheet(title_style)
        self.group_title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.title_box.addWidget(self.group_title)

        self.challenge_title = QLabel()
        self.challenge_title.setStyleSheet(text_style)
        self.challenge_title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.title_box.addWidget(self.challenge_title)

        self.title_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.volume_btn = QPushButton()
        self.volume_btn.setFixedSize(40, 40)

        icon = QIcon()

        if self.player.volume() == 1.0:
            icon.addPixmap(QPixmap(":/volume-high"), QIcon.Mode.Normal, QIcon.State.Off)
        else:
            icon.addPixmap(QPixmap(":/volume-xmark"), QIcon.Mode.Normal, QIcon.State.Off)

        self.volume_btn.setIcon(icon)

        self.title_wrap.addWidget(self.volume_btn)

        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QMinimum))

        # === Explanation ===

        self.explanation_box = QHBoxLayout()
        self._layout.addLayout(self.explanation_box)

        self.qwe = self._new_webengine()
        self.qwe.setHidden(True)
        self.explanation_box.addWidget(self.qwe)

        # === Gfx ===

        self.gfx_box = QHBoxLayout()
        self._layout.addLayout(self.gfx_box)

        self.gfx_label = QLabel()
        self.gfx_label.setHidden(True)
        self.gfx_box.addWidget(self.gfx_label)

        # === Question ===

        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QMinimum))

        self.question_box = QHBoxLayout()
        self._layout.addLayout(self.question_box)

        self.question_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.question_text = QLabel()
        self.question_text.setMinimumWidth(400)
        self.question_text.setSizePolicy(QExpanding, QMinimum)
        self.question_text.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.question_text.setStyleSheet(text_style)
        self.question_text.setWordWrap(True)

        self.question_box.addWidget(self.question_text)

        self.question_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self._layout.addItem(QSpacerItem(0, 20, QMinimum, QExpanding))

        # === Horizontal Line ===

        self.question_sep_line = QFrame()
        self.question_sep_line.setFrameShape(QFrame.Shape.HLine)
        self.question_sep_line.setFrameShadow(QFrame.Shadow.Sunken)
        self.question_sep_line.setLineWidth(1)
        self._layout.addWidget(self.question_sep_line)

        # === Answer Text ===

        self.answer_top_spacer = QSpacerItem(0, 20, QMinimum, QMinimum)
        self._layout.addItem(self.answer_top_spacer)

        self.answer_text_box = QHBoxLayout()
        self._layout.addLayout(self.answer_text_box)

        self.answer_text_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.answer_text = QLabel()
        self.answer_text.setStyleSheet(text_style)
        self.answer_text_box.addWidget(self.answer_text)

        self.answer_listen = QPushButton()

        self.answer_listen.setFixedSize(40, 40)
        self.answer_listen.setHidden(True)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/play-regular"), QIcon.Mode.Normal, QIcon.State.Off)
        self.answer_listen.setIcon(icon)

        self.answer_text_box.addWidget(self.answer_listen)

        self.answer_text_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        # === Answer Buttons ===

        self.answer_sentence_wrap = QHBoxLayout()
        self._layout.addLayout(self.answer_sentence_wrap)

        self.answer_sentence_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.answer_sentence_box = QHBoxLayout()
        self.answer_sentence_wrap.addLayout(self.answer_sentence_box)

        self.answer_sentence_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        # === Choices ===

        self.choices_top_spacer = QSpacerItem(0, 50, QMinimum, QMinimum)
        self._layout.addItem(self.choices_top_spacer)

        self.choices_wrap = QHBoxLayout()
        self._layout.addLayout(self.choices_wrap)

        self.choices_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.choices_box = QHBoxLayout()
        self.choices_wrap.addLayout(self.choices_box)

        self.choices_wrap.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        # === Footer ===

        # Double spacer helps spacing proportions
        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QExpanding))
        self._layout.addItem(QSpacerItem(0, 10, QMinimum, QExpanding))

        self.footer_box = QHBoxLayout()
        self._layout.addLayout(self.footer_box)

        self.close_btn = QPushButton("Close")
        self.close_btn.setFixedSize(80, 40)
        self.footer_box.addWidget(self.close_btn)

        self.footer_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.message = QLabel()
        self.message.setStyleSheet(title_style)
        self.message.setBaseSize(100, 40)
        self.footer_box.addWidget(self.message)

        self.footer_box.addItem(QSpacerItem(10, 0, QExpanding, QMinimum))

        self.continue_btn = QPushButton("Continue")
        self.continue_btn.setEnabled(False)
        self.continue_btn.setFixedSize(80, 40)
        self.footer_box.addWidget(self.continue_btn)


    def _new_webengine(self) -> QWebEngineView:
        qwe = QWebEngineView()
        qwe.setPage(ReaderWebEnginePage(self))

        qwe.setMinimumHeight(500)
        qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

        return qwe


    def _render_content(self, md_text: str):
        html_content = markdown.markdown(md_text)

        css = """
        body { max-width: 100ex; }
        #ssp_main { padding-bottom: 0pt; }
        """

        html = html_page(html_content, self._app_data.api_url, css_extra=css)
        self.set_qwe_html(html)


    def set_qwe_html(self, html: str):
        # Around above this size, QWebEngineView doesn't display the HTML with .setHtml()
        size_limit = 0.8 * 1024 * 1024

        if len(html) < size_limit:
            try:
                self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))
            except Exception as e:
                logger.error("set_qwe_html() : %s" % e)
        else:
            logger.error("HTML too long")


    def _setup_choices_box(self):
        self.choices_box = QHBoxLayout()
        self._layout.addLayout(self.choices_box)


    def _remove_choice_buttons(self):
        for i in self.choice_buttons:
            i.do_remove()

        while True:
            item = self.choices_box.takeAt(0)
            if item is None:
                break
            else:
                self.choices_box.removeItem(item)

        self.choice_buttons = []


    def _remove_answer_buttons(self):
        for i in self.answer_buttons:
            i.do_remove()

        while True:
            item = self.answer_sentence_box.takeAt(0)
            if item is None:
                break
            else:
                self.answer_sentence_box.removeItem(item)

        self.answer_buttons = []


    def _reset_challenge_content(self):
        self.group_title.setText("")
        self.challenge_title.setText("")
        self.message.setText("")
        self.gfx_label.setHidden(True)
        self.question_text.setText("")
        self.question_sep_line.setHidden(False)
        self.answer_top_spacer.changeSize(0, 20)
        self.answer_text.setText("")
        self.answer_text.setHidden(False)
        self.answer_listen.setHidden(True)
        self.choices_top_spacer.changeSize(0, 50)
        self.set_qwe_html("")
        self.qwe.setHidden(True)

        self.continue_btn.setEnabled(False)

        self._remove_choice_buttons()
        self._remove_answer_buttons()


    def _set_current_group_title(self):
        group_title = str(self.current_challenge.group.name)
        self.group_title.setText(f"{group_title} ({self.progress_in_remaining}/{len(self.challenges)})")


    def _load_challenge_content(self):
        self._reset_challenge_content()

        self._set_current_group_title()

        ch = self.current_challenge

        self.challenge_title.setText(str(ch.challenge_type))

        if ch.challenge_type == PaliChallengeType.TranslateFromEnglish.value or \
           ch.challenge_type == PaliChallengeType.TranslateFromPali:

            self.continue_btn.setText("Check")
            self.continue_btn.setEnabled(True)

            self._load_question(ch)
            self._load_answer(ch)

        elif ch.challenge_type == PaliChallengeType.Vocabulary.value:

            self.continue_btn.setText("Continue")
            self.continue_btn.setEnabled(False)

            self._load_question(ch)
            self._load_answer(ch)

        elif ch.challenge_type == PaliChallengeType.Explanation.value:
            self.question_sep_line.setHidden(True)
            self.answer_text.setHidden(True)
            self.answer_top_spacer.changeSize(0, 0)
            self.choices_top_spacer.changeSize(0, 0)

            self.continue_btn.setText("Continue")
            self.continue_btn.setEnabled(True)

            self._load_explanation(ch)


    def _vocab_from_current_course(self, except_words: Optional[List[str]] = None) -> Dict[str, PaliItem]:
        def _is_vocab(x: UChallenge) -> bool:
            return (str(x.challenge_type) == PaliChallengeType.Vocabulary.value)

        course = self.current_challenge.course

        vocab = list(filter(_is_vocab, course.challenges)) # type: ignore

        ret: Dict[str, PaliItem] = dict()

        for i in vocab:
            r: List[PaliItem] = json.loads(str(i.answers_json))
            r[0]['uuid'] = str(uuid.uuid4())
            word = r[0]["text"].strip().lower()
            if except_words and word in except_words:
                continue
            ret[word] = r[0]

        return ret


    def _current_course_dir(self) -> Optional[Path]:
        if self.current_challenge is None:
            return None

        d = self.current_challenge.course.course_dirname
        if d is not None:
            course_dir = COURSES_DIR.joinpath(d)
        else:
            course_dir = None

        return course_dir

    def _load_explanation(self, ch: UChallenge):
        self.qwe.setHidden(False)
        self._render_content(str(ch.explanation_md))


    def _load_question(self, ch: UChallenge):
        question: PaliItem = json.loads(str(ch.question_json))

        d = ch.course.course_dirname
        if d is not None:
            course_dir = COURSES_DIR.joinpath(d)
        else:
            course_dir = None

        self.question_text.setText(question['text'])

        if course_dir and question['gfx']:
            gfx_path = course_dir.joinpath(question['gfx'])
            if not gfx_path.exists():
                logger.warn("Gfx missing: " + str(gfx_path))
                return

            self.gfx_label.setHidden(False)
            pixmap = QPixmap(str(gfx_path))
            # Scale to fixed height
            h = 150
            w = int((h/pixmap.height()) * pixmap.width())
            self.gfx_label.setFixedSize(w, h)
            self.gfx_label.setScaledContents(True)
            self.gfx_label.setPixmap(pixmap)


    def _load_answer(self, ch: UChallenge):
        answers: List[PaliItem] = json.loads(str(ch.answers_json))

        choices: List[PaliItem] = []

        d = ch.course.course_dirname
        if d is not None:
            course_dir = COURSES_DIR.joinpath(d)
        else:
            course_dir = None

        vocab = self._vocab_from_current_course()
        except_words = []

        if ch.challenge_type == PaliChallengeType.Vocabulary.value:
            choices.append(answers[0])

        elif ch.challenge_type == PaliChallengeType.TranslateFromEnglish.value or \
             ch.challenge_type == PaliChallengeType.TranslateFromPali.value:

            for i in answers[0]['text'].split("|"):
                w = i.strip().lower()
                if w in vocab.keys() and vocab[w]['audio']:
                    audio = vocab[w]['audio']
                else:
                    audio = None

                choices.append(PaliItem(text=i.strip(), audio=audio, gfx=None, uuid=str(uuid.uuid4())))
                except_words.append(w)

        for i in except_words:
            if i in vocab.keys():
                del vocab[i]

        used = []
        tries = 0
        added = 0
        words = list(vocab.keys())

        while added < 3 and tries < 5:
            n = random.randrange(0, len(words))
            if n in used:
                tries += 1
                continue
            used.append(n)
            choices.append(vocab[words[n]])
            added += 1
            tries = 0

        random.shuffle(choices)

        for i in choices:
            btn = ChoiceButton(i, course_dir)

            btn.choice_clicked.connect(partial(self._handle_choice_clicked))
            btn.play_audio.connect(partial(self._play_audio))

            self.choice_buttons.append(btn)
            self.choices_box.addWidget(btn)


    def _play_answer(self, pali_item: PaliItem):
        if pali_item['audio'] is None:
            return

        course_dir = self._current_course_dir()
        if course_dir is None:
            return

        audio_path = course_dir.joinpath(pali_item['audio'])
        if audio_path.exists():
            self.answer_listen.setHidden(False)
            self.answer_listen.clicked.connect(partial(self._play_audio, audio_path))
            self._play_audio(audio_path)


    def _current_challenge_success(self):
        self.current_challenge.studied_at = func.now() # type: ignore
        if self.current_challenge.level is None:
            self.current_challenge.level = 1 # type: ignore
        else:
            n = int(self.current_challenge.level) # type: ignore
            self.current_challenge.level = n+1 # type: ignore

        self._app_data.db_session.commit()

        self.progress_in_remaining += 1

        group = self.current_group
        self._app_data.pali_groups_stats[group.metadata.schema][group.id]['completed'] += 1


    def _current_challenge_fail(self):
        self.current_challenge.studied_at = func.now() # type: ignore
        self.current_challenge.level = 0 # type: ignore
        self._app_data.db_session.commit()


    def _answer_success(self, answer: str, pali_item: PaliItem):
        self.answer_text.setText(answer)

        self._remove_answer_buttons()
        self._remove_choice_buttons()

        self._play_answer(pali_item)

        self.message.setText("Correct!")

        if self.continue_btn.text() == "Check":
            self.continue_btn.setText("Continue")

        self.continue_btn.setEnabled(True)

        self._current_challenge_success()

        for i in self.choice_buttons:
            i.setEnabled(False)


    def _answer_fail(self):
        self.message.setText("Incorrect answer, try again.")
        if self.continue_btn.text() == "Continue":
            self.continue_btn.setEnabled(False)


    def _handle_remove_choice(self, item: PaliItem):
        if item['uuid'] is None:
            return

        idx: Optional[int] = None
        for n, i in enumerate(self.choice_buttons):
            if i.pali_item['uuid'] == item['uuid']:
                idx = n
                i.do_remove()
                break

        if idx is None:
            return

        a = self.choices_box.takeAt(idx)

        if a is not None:
            self.choices_box.removeItem(a)

        del self.choice_buttons[idx]


    def _handle_remove_answer(self, item: PaliItem):
        if item['uuid'] is None:
            return

        idx: Optional[int] = None
        for n, i in enumerate(self.answer_buttons):
            if i.pali_item['uuid'] == item['uuid']:
                idx = n
                i.do_remove()
                break

        if idx is None:
            return

        a = self.answer_sentence_box.takeAt(idx)

        if a is not None:
            self.answer_sentence_box.removeItem(a)

        del self.answer_buttons[idx]


    def _check_answer_vocabulary(self, item: PaliItem):
        items: List[PaliItem] = json.loads(str(self.current_challenge.answers_json))
        correct_answers = list(map(lambda x: x['text'], items))

        answer = item['text']
        if answer in correct_answers:
            idx = correct_answers.index(answer)
            self._answer_success(answer, items[idx])

        else:
            self._answer_fail()


    def _check_answer_translate(self):
        items: List[PaliItem] = json.loads(str(self.current_challenge.answers_json))

        def _fmt(x: PaliItem) -> str:
            s = re.sub(r' *\| *', ' ', x['text'])
            s = re.sub(r' +', ' ', s)
            return s

        correct_answers = list(map(_fmt, items))

        b: List[str] = list(map(lambda x: x.text(), self.answer_buttons))
        answer = re.sub(r' +', ' ', " ".join(b))

        if answer in correct_answers:
            idx = correct_answers.index(answer)
            self._answer_success(answer, items[idx])

        else:
            self._answer_fail()


    def _show_info_message(self, msg: str, title: str = "Message"):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle(title)
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Ok)

        _ = box.exec()


    def _group_completed(self):
        msg = "<p>Completed: %s</p>" % str(self.current_group.name)
        self._show_info_message(msg, "Completed")

        self._handle_close()


    def _handle_answer_clicked(self, answer_item: PaliItem):
        choice_item = PaliItem(
            text = answer_item['text'],
            audio = answer_item['audio'],
            gfx = None,
            uuid = str(uuid.uuid4()),
        )

        course_dir = self._current_course_dir()
        btn = ChoiceButton(pali_item = choice_item, course_dir = course_dir, enable_audio = True)
        btn.choice_clicked.connect(partial(self._handle_choice_clicked))
        btn.play_audio.connect(partial(self._play_audio))

        self.choice_buttons.append(btn)
        self.choices_box.addWidget(btn)

        self._handle_remove_answer(answer_item)


    def _handle_choice_clicked(self, choice_item: PaliItem):
        if self.current_challenge.challenge_type == PaliChallengeType.Vocabulary:

            self._check_answer_vocabulary(choice_item)

        elif self.current_challenge.challenge_type == PaliChallengeType.TranslateFromEnglish or \
             self.current_challenge.challenge_type == PaliChallengeType.TranslateFromPali:

            answer_item = PaliItem(
                text = choice_item['text'],
                audio = choice_item['audio'],
                gfx = None,
                uuid = str(uuid.uuid4()),
            )

            btn = ChoiceButton(pali_item = answer_item, enable_audio = False)
            btn.choice_clicked.connect(partial(self._handle_answer_clicked))

            self.answer_buttons.append(btn)
            self.answer_sentence_box.addWidget(btn)

            self._handle_remove_choice(choice_item)

        else:
            logger.info(f"Unknown challange type: {choice_item}")


    def _handle_continue(self):
        if self.continue_btn.text() == "Continue":
            if self.current_challenge.challenge_type == PaliChallengeType.Explanation:
                self._current_challenge_success()

            if self._set_next_challenge():
                self._load_challenge_content()

            else:
                self._group_completed()

        else:
            self._check_answer_translate()


    def _handle_toggle_volume(self):
        volume = self.player.volume()
        icon = QIcon()

        print(volume)

        if volume == 1.0:
            volume = 0.0
            self.player.setMuted(True)
            self.player.setVolume(volume)
            icon.addPixmap(QPixmap(":/volume-xmark"), QIcon.Mode.Normal, QIcon.State.Off)

        else:
            volume = 1.0
            self.player.setMuted(False)
            self.player.setVolume(volume)
            icon.addPixmap(QPixmap(":/volume-high"), QIcon.Mode.Normal, QIcon.State.Off)

        self.volume_btn.setIcon(icon)

        self._app_data.app_settings['audio_volume'] = volume
        self._app_data._save_app_settings()


    def _handle_close(self):
        self._app_data._save_pali_groups_stats(self.current_group.metadata.schema)
        self.player.stop()
        self.finished.emit()
        self.close()


    def _connect_signals(self):
        self.close_btn.clicked.connect(self._handle_close)

        self.continue_btn.clicked.connect(partial(self._handle_continue))

        self.volume_btn.clicked.connect(partial(self._handle_toggle_volume))
