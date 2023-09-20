from pathlib import Path
import smtplib, ssl
from email import encoders
from email.mime.base import MIMEBase
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from typing import List

from simsapa.layouts.gui_types import SmtpLoginData

class SimsapaSMTP(smtplib.SMTP):
    """A wrapper for handling SMTP connections."""

    def __init__(self, smtp_data: SmtpLoginData):
        super().__init__(host=smtp_data['host'], port=smtp_data['port_tls'])
        context = ssl.create_default_context()
        self.starttls(context=context)
        self.login(user=smtp_data['user'], password=smtp_data['password'])

    def send_message(self,
                     from_addr: str,
                     to_addr: str,
                     msg: str,
                     subject: str,
                     attachment_paths: List[Path]):

        msg_root = MIMEMultipart()
        msg_root['Subject'] = subject
        msg_root['From'] = from_addr
        msg_root['To'] = to_addr

        msg_alternative = MIMEMultipart('alternative')
        msg_root.attach(msg_alternative)
        msg_alternative.attach(MIMEText(msg))

        if len(attachment_paths) > 0:
            for path in attachment_paths:
                prt = MIMEBase('application', "octet-stream")
                prt.set_payload(open(path, "rb").read())
                encoders.encode_base64(prt)
                filename = path.name.replace('"', '')
                prt.add_header('Content-Disposition', f'attachment; filename="{filename}"')
                msg_root.attach(prt)

        self.sendmail(from_addr, to_addr, msg_root.as_string())
