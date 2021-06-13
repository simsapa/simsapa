INSERT INTO dictionaries VALUES(1,'Study','Personal Study',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'2021-06-09 15:36:10',NULL);
INSERT INTO dict_words VALUES(1,1,'dhamma','dhamma',NULL,NULL,NULL,NULL,NULL,'the truth',NULL,NULL,NULL,NULL,NULL,NULL,NULL,'2021-06-09 15:37:27',NULL);
INSERT INTO decks VALUES(1,'Simsapa','2021-06-10 06:56:00',NULL);
INSERT INTO memos VALUES(1,1,replace('{\n  "Front": "Who is criticizing the Buddha?",\n  "Back": "the wanderer Suppiya"\n}','\n',char(10)),NULL,NULL,NULL,'2021-06-10 06:55:52',NULL);
INSERT INTO memos VALUES(2,1,replace('{\n  "Front": "Who is praising the Buddha?",\n  "Back": "the brahmin student Brahmadatta"\n}','\n',char(10)),NULL,NULL,NULL,'2021-06-10 07:03:02',NULL);
INSERT INTO memos VALUES(3,1,replace('{\n  "Front": "DN 2: Who is the sutta spoken to?",\n  "Back": "King Ajātasattu"\n}','\n',char(10)),NULL,NULL,NULL,'2021-06-10 07:39:00',NULL);
INSERT INTO memo_associations VALUES(1,1,'appdata.suttas',1,NULL,NULL);
INSERT INTO memo_associations VALUES(2,2,'appdata.suttas',1,NULL,NULL);
INSERT INTO memo_associations VALUES(3,3,'appdata.suttas',2,NULL,NULL);
